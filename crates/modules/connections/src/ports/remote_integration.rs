use floe_access::VerifiedGatewayBinding;
use floe_context_contract::{ConnectionId,ConnectorId,SourceAuthority};
use floe_execution::BoxFuture;
use serde::{Deserialize,Serialize};
use uuid::Uuid;
use crate::{ConnectionResource,OperationScope,SourceConnection,GatewaySummary};
#[derive(Clone,Copy,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum IntegrationError{InvalidInput,ForeignIdentity,Conflict,NotFound,Unavailable,Cancelled,DeadlineExceeded,InvalidResponse}
#[derive(Clone,Debug)]
pub struct IntegrationCatalogQuery{pub gateway_ref:Uuid,pub expected:VerifiedGatewayBinding}
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationDescriptor{pub connector_id:ConnectorId,pub display_name:String,pub category:String,pub setup_kind:IntegrationSetupKind,pub available:bool,pub connected:bool,pub catalog_revision:u64,pub initial_selection:IntegrationSelection}
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(tag="kind",content="resources",rename_all="snake_case",deny_unknown_fields)]
pub enum IntegrationSelection{GatewayManaged,SelectedResources(Vec<floe_context_contract::ResourceHandle>)}
#[derive(Clone,Copy,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum IntegrationSetupKind{BrowserAuthorization,DeviceCode,GatewayManagedSecret,NativePermission}
#[derive(Clone,Debug)]
pub struct IntegrationCatalog{pub gateway_ref:Uuid,pub binding:VerifiedGatewayBinding,pub entries:Vec<IntegrationDescriptor>}
#[derive(Clone,Debug)]
pub struct BeginIntegration{pub operation_id:Uuid,pub gateway_ref:Uuid,pub expected:VerifiedGatewayBinding,pub connector_id:ConnectorId,pub expected_catalog_revision:u64,pub selection:IntegrationSelection}
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationOperationRef{pub operation_id:Uuid,pub gateway_ref:Uuid,pub expected:VerifiedGatewayBinding,pub connector_id:ConnectorId,pub remote_operation_ref:Uuid,pub remote_revision:u64}
#[derive(Clone,Copy,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum RemoteIntegrationState{AwaitingUser,Pending,Connected,Failed,Cancelled}
#[derive(Clone,Debug)]
pub struct IntegrationOperation{pub reference:IntegrationOperationRef,pub state:RemoteIntegrationState,pub connection_id:ConnectionId,pub management_launch:Option<ValidatedManagementLaunch>,pub source:Option<RemoteIntegrationSource>}
#[derive(Clone,Debug)]
pub struct RemoteIntegrationSource{pub execution_owner_id:floe_context_contract::ExecutionOwnerId,pub source_authority:SourceAuthority,pub resources:Vec<ConnectionResource>,pub provider_revision:u64}
#[derive(Clone,Debug)]
pub struct CancelIntegration{pub operation:IntegrationOperationRef,pub expected_remote_revision:u64}
#[derive(Clone,Debug)]
pub struct ConfigureIntegration{pub operation_id:Uuid,pub gateway_ref:Uuid,pub expected:VerifiedGatewayBinding,pub connector_id:ConnectorId,pub connection_id:ConnectionId,pub expected_remote_revision:u64,pub selection:IntegrationSelection}
#[derive(Clone,Debug)]
pub struct DisconnectIntegration{pub operation_id:Uuid,pub gateway_ref:Uuid,pub expected:VerifiedGatewayBinding,pub connector_id:ConnectorId,pub connection_id:ConnectionId,pub expected_remote_revision:u64}
#[derive(Clone,Debug)]
pub struct IntegrationSnapshot{pub connection_id:ConnectionId,pub revision:u64,pub disconnected:bool,pub cleanup_complete:bool}
#[derive(Clone,Copy,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum LaunchPurpose{ManageGateway,AuthorizeIntegration}
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatedManagementLaunch{pub action_ref:Uuid,pub purpose:LaunchPurpose,pub validated_url:String,pub expires_at:chrono::DateTime<chrono::Utc>}
#[derive(Clone,Debug)]
pub struct ManagementLaunchRequest{pub operation_id:Uuid,pub gateway_ref:Uuid,pub expected:VerifiedGatewayBinding,pub expected_binding_generation:u64,pub purpose:LaunchPurpose}
pub trait RemoteIntegrationPort:Send+Sync{
    fn list<'a>(&'a self,query:IntegrationCatalogQuery,scope:&'a OperationScope)->BoxFuture<'a,Result<IntegrationCatalog,IntegrationError>>;
    fn begin<'a>(&'a self,command:BeginIntegration,scope:&'a OperationScope)->BoxFuture<'a,Result<IntegrationOperation,IntegrationError>>;
    fn observe<'a>(&'a self,operation:&'a IntegrationOperationRef,scope:&'a OperationScope)->BoxFuture<'a,Result<IntegrationOperation,IntegrationError>>;
    fn cancel<'a>(&'a self,command:CancelIntegration,scope:&'a OperationScope)->BoxFuture<'a,Result<IntegrationOperation,IntegrationError>>;
    fn configure<'a>(&'a self,command:ConfigureIntegration,scope:&'a OperationScope)->BoxFuture<'a,Result<IntegrationSnapshot,IntegrationError>>;
    fn disconnect<'a>(&'a self,command:DisconnectIntegration,scope:&'a OperationScope)->BoxFuture<'a,Result<IntegrationSnapshot,IntegrationError>>;
    fn management_launch<'a>(&'a self,request:ManagementLaunchRequest,scope:&'a OperationScope)->BoxFuture<'a,Result<ValidatedManagementLaunch,IntegrationError>>;
}
#[derive(Clone,Debug)]
pub enum GatewayObservation{
    Paired{summary:GatewaySummary,binding:VerifiedGatewayBinding},
    RepairRequired{summary:GatewaySummary,slot_digest:Option<[u8;32]>,expectation:floe_access::GatewayCredentialExpectation},
}
impl GatewayObservation{pub fn summary(&self)->&GatewaySummary{match self{Self::Paired{summary,..}|Self::RepairRequired{summary,..}=>summary}}}
#[derive(Clone,Debug)]
pub enum GatewayForgetExpectation{Paired(VerifiedGatewayBinding),Unreadable{gateway_ref:Uuid,revision:u64,slot_digest:Option<[u8;32]>,expectation:floe_access::GatewayCredentialExpectation}}
/// Public metadata and explicit forgetting at the real secure-store boundary.
pub trait GatewayRegistry:Send+Sync{
    fn current<'a>(&'a self,person:floe_kernel::PersonId,device:&'a str)->BoxFuture<'a,Result<Option<GatewayObservation>,crate::PairingError>>;
    fn forget<'a>(&'a self,operation_id:Uuid,expected:GatewayForgetExpectation)->BoxFuture<'a,Result<GatewaySummary,crate::PairingError>>;
    fn forgotten<'a>(&'a self,operation_id:Uuid)->BoxFuture<'a,Result<Option<GatewaySummary>,crate::PairingError>>;
}
#[derive(Clone,Debug,Eq,PartialEq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCatalogObservation{pub source:SourceConnection,pub resources:Vec<ConnectionResource>,pub catalog_digest:[u8;32],pub catalog_complete:bool}
pub trait SourceCatalogPort:Send+Sync{
    fn inspect<'a>(&'a self,actor:&'a floe_kernel::OwnerActor,source:&'a SourceConnection,scope:&'a OperationScope)->BoxFuture<'a,Result<SourceCatalogObservation,floe_kernel::AgentFailure>>;
}

#[derive(Clone,Debug)]
pub struct NativeSetupRequest{pub operation_id:Uuid,pub connector_id:ConnectorId,pub connection_id:ConnectionId,pub source_revision:u64}
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum NativeSetupState{Completed,Denied,Unavailable}
#[derive(Clone,Debug)]
pub struct NativeSetupObservation{pub operation_id:Uuid,pub connector_id:ConnectorId,pub state:NativeSetupState}
pub trait NativeSourceSetupPort:Send+Sync{
    fn request_permission<'a>(&'a self,actor:&'a floe_kernel::OwnerActor,request:NativeSetupRequest,scope:&'a OperationScope)->BoxFuture<'a,Result<NativeSetupObservation,floe_kernel::AgentFailure>>;
}
