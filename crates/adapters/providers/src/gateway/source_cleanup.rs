//! Exact remote source disconnect receipt reconciliation. The revocation receipt
//! already exists before this adapter may contact the Gateway.
use floe_access::{GrantCommitKind,GrantCommitReceipt};
use floe_agent_contract::{AgentFailure,BoxFuture};
use floe_connections::{SourceCleanup,SourceCleanupOutcome,SourceOperationKind,SourceOperationPhase,SourceOperationRecord};
use floe_execution::ExecutionScope;
use serde::Deserialize;
use super::{credentials::GatewayCredentialStore,http::GatewayHttpTransport};
pub struct GatewaySourceCleanup{store:GatewayCredentialStore}
impl GatewaySourceCleanup{pub fn new(store:GatewayCredentialStore)->Self{Self{store}}}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisconnectReceipt{schema_version:u32,operation_id:uuid::Uuid,person_id:String,device_id:String,connection_id:String,connector_id:String,connection_revision:u64,cleanup_state:String}
impl SourceCleanup for GatewaySourceCleanup{
    fn disconnect<'a>(&'a self,operation:&'a SourceOperationRecord,receipt:&'a GrantCommitReceipt,scope:&'a ExecutionScope)->BoxFuture<'a,Result<SourceCleanupOutcome,AgentFailure>>{Box::pin(async move{
        if operation.kind!=SourceOperationKind::ConnectionDisconnect||receipt.kind!=GrantCommitKind::Disconnect
            ||!operation.matches_evidence(&receipt.reservation)||operation.phase.receipt_id()!=Some(receipt.commit_id)
            ||!matches!(operation.phase,SourceOperationPhase::CleaningUp{..}|SourceOperationPhase::RepairRequired{..}){return Err(AgentFailure::PolicyDenied)}
        let expected=receipt.reservation.source.gateway.as_ref().ok_or(AgentFailure::PolicyDenied)?;
        let revision=receipt.reservation.source.provider_revision.ok_or(AgentFailure::PolicyDenied)?;
        let connection=self.store.load(&expected.person_id,&expected.device_id).await.map_err(|_|AgentFailure::PolicyDenied)?.ok_or(AgentFailure::PolicyDenied)?;
        if &connection.binding!=expected{return Err(AgentFailure::PolicyDenied)}
        let connector=operation.expected.source.connector().as_str();
        if connector.is_empty()||!connector.bytes().all(|byte|byte.is_ascii_alphanumeric()||matches!(byte,b'.'|b'_'|b'-')){return Err(AgentFailure::InvalidInput)}
        let body=serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation_id":operation.operation_id,"connection_id":operation.expected.source.connection_id(),"connection_revision":revision})).map_err(|_|AgentFailure::InvalidInput)?;
        let http=GatewayHttpTransport::new()?;
        let exchange=http.request(&connection.endpoint,Some(&connection.bearer),reqwest::Method::POST,&format!("/v1/connectors/{connector}/disconnect"),Some(body),scope.deadline(),scope.cancellation()).await;
        let (status,bytes)=match exchange{Ok(value)=>value,Err(AgentFailure::Cancelled|AgentFailure::DeadlineExceeded|AgentFailure::ServerModelTimeout|AgentFailure::ServerModelUnavailable)=>return Ok(SourceCleanupOutcome::Uncertain),Err(error)=>return Err(error)};
        if status!=200{return Err(match status{409=>AgentFailure::Conflict,401|403=>AgentFailure::PolicyDenied,_=>AgentFailure::CapabilityUnavailable})}
        super::json::strict_json_bytes(&bytes,65536)?;
        let actual:DisconnectReceipt=serde_json::from_slice(&bytes).map_err(|_|AgentFailure::PolicyDenied)?;
        if actual.schema_version!=1||actual.operation_id!=operation.operation_id||actual.person_id!=expected.person_id||actual.device_id!=expected.device_id
            ||actual.connection_id!=operation.expected.source.connection_id().as_str()||actual.connector_id!=connector||actual.connection_revision!=revision{return Err(AgentFailure::PolicyDenied)}
        let current=self.store.current_binding(&expected.person_id,&expected.device_id).await.map_err(|_|AgentFailure::PolicyDenied)?;
        if current.as_ref()!=Some(expected){return Err(AgentFailure::PolicyDenied)}
        match actual.cleanup_state.as_str(){"completed"=>Ok(SourceCleanupOutcome::Completed),"pending"=>Ok(SourceCleanupOutcome::Uncertain),_=>Err(AgentFailure::PolicyDenied)}
    })}
}
