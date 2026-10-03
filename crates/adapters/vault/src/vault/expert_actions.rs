//! Physical Task receipt/artifact reader for Actions. Immutable Task provenance
//! is validated by Experts; proposal and effect policy remain Actions-owned.

use std::sync::Arc;
use floe_actions::{ExpertProposalEvidence,ExpertProposalReader,ExpertCalendarProposal,EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE};
use floe_agent_contract::{ArtifactPart,DependencyCoverage,TaskExecutionReceiptRef};
use floe_execution::{BoxFuture,ExecutionScope};
use floe_experts::{TaskRecord,TaskRepository};
use floe_kernel::{AgentFailure,OwnerActor,PersonId};
use uuid::Uuid;

pub struct VaultExpertProposalReader {
    tasks:Arc<dyn TaskRepository>,
}

impl VaultExpertProposalReader {
    pub fn new(tasks:Arc<dyn TaskRepository>)->Self{Self{tasks}}
}

impl ExpertProposalReader for VaultExpertProposalReader {
    fn read<'a>(&'a self,actor:&'a OwnerActor,reference:&'a TaskExecutionReceiptRef,artifact_id:Uuid,scope:&'a ExecutionScope)
        ->BoxFuture<'a,Result<ExpertProposalEvidence,AgentFailure>>{
        Box::pin(async move{
            actor.validate()?;
            reference.validate()?;
            let receipt=scope.run(self.tasks.read_execution_receipt(reference.clone())).await?;
            let record=scope.run(self.tasks.get(reference.execution.task_id)).await?.ok_or(AgentFailure::NotFound)?;
            if record.receipt.as_ref()!=Some(&receipt){return Err(AgentFailure::StorageUnavailable);}
            decode_task_proposal(&record,reference,artifact_id,actor.person_id,&actor.device_id)
        })
    }
}

/// The encrypted Actions admission transaction may call this after loading the
/// exact TaskRecord in that same transaction. No fresh Registry lookup is used.
pub(super) fn decode_task_proposal(record:&TaskRecord,reference:&TaskExecutionReceiptRef,artifact_id:Uuid,person_id:PersonId,device_id:&str)
    ->Result<ExpertProposalEvidence,AgentFailure>{
    let trusted=floe_experts::validate_task_artifact(record,reference,artifact_id,person_id,device_id)?;
    let [ArtifactPart::Data{media_type,data}]=trusted.artifact.parts.as_slice() else{return Err(AgentFailure::PolicyDenied)};
    if media_type!=EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE || data.len()>16_384 {return Err(AgentFailure::PolicyDenied);}
    let proposal:ExpertCalendarProposal=serde_json::from_str(data).map_err(|_|AgentFailure::InvalidInput)?;
    proposal.validate()?;
    if proposal.person_id!=person_id || proposal.task_id!=reference.execution.task_id.as_uuid()
        || proposal.invocation_id!=trusted.invocation_key.as_uuid() || proposal.instance_id!=trusted.admission.registry_instance_id
        || proposal.assignment_id!=trusted.admission.assignment_id || proposal.package!=trusted.admission.package
        || proposal.data_class!=floe_context_contract::DataClass::Personal {
        return Err(AgentFailure::PolicyDenied);
    }
    let DependencyCoverage::Dependent{dependencies}=&trusted.artifact.coverage else{return Err(AgentFailure::PolicyDenied)};
    let [dependency]=dependencies.as_slice() else{return Err(AgentFailure::PolicyDenied)};
    if dependency.person_id()!=person_id || dependency.observation_id()!=proposal.evidence_id
        || dependency.consumer().identifier()!=proposal.package.id || dependency.source().connector().as_str()!="calendar.event_kit"
        || !trusted.selection.requirements.iter().any(|requirement|requirement.capability=="calendar.timeline" && requirement.selected.iter().any(|selected|
            selected.connector_id==*dependency.source().connector() && selected.connection_id==dependency.source().connection_id()
            && selected.execution_owner_id==*dependency.source().execution_owner() && dependency.resources().contains(&selected.resource))) {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(ExpertProposalEvidence{receipt:reference.clone(),artifact_id,proposal,dependency:dependency.clone(),installation_id:trusted.admission.installation_id,
        assignment_id:trusted.admission.assignment_id,definition_revision:trusted.admission.definition_revision,invocation_id:trusted.invocation_key.as_uuid()})
}
