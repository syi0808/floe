//! Mechanical forwarding to the one admitted Experts owner.
use crate::{AppComposition, CallerContext};
use floe_kernel::{AgentFailure, CommandId};
use uuid::Uuid;

pub enum ExpertCommand {
    SetInstallationEnabled {
        installation_ref: Uuid,
        expected_revision: u64,
        enabled: bool,
    },
    PrepareBindingReview {
        assignment_ref: Uuid,
        requirement_ref: String,
        expected_binding_revision: u64,
    },
    ReplaceBinding {
        review_ref: floe_experts::BindingReviewRef,
        expected_binding_revision: u64,
        candidate_refs: Vec<Uuid>,
    },
}
pub enum ExpertQuery {
    Directory,
    InspectBinding {
        assignment_ref: Uuid,
        requirement_ref: String,
    },
    InspectBindingReview {
        review_ref: floe_experts::BindingReviewRef,
    },
}
pub enum ExpertCommandResult {
    Directory(floe_experts::ExpertDirectorySnapshot),
    BindingReview(floe_experts::BindingReview),
}
pub enum ExpertQueryResult {
    Directory(floe_experts::ExpertDirectorySnapshot),
    Binding(floe_experts::BindingInspection),
    BindingReview(floe_experts::BindingReview),
}
pub trait ExpertCommands {
    fn expert_command(
        &self,
        caller: &CallerContext,
        command_id: Uuid,
        command: ExpertCommand,
    ) -> Result<ExpertCommandResult, AgentFailure>;
}
pub trait ExpertQueries {
    fn expert_query(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
        query: ExpertQuery,
    ) -> Result<ExpertQueryResult, AgentFailure>;
}
impl ExpertCommands for AppComposition {
    fn expert_command(
        &self,
        caller: &CallerContext,
        command_id: Uuid,
        command: ExpertCommand,
    ) -> Result<ExpertCommandResult, AgentFailure> {
        let command_id = CommandId::from_uuid(command_id).ok_or(AgentFailure::InvalidInput)?;
        let owners = self.ready_owners(caller)?;
        let actor = caller.owner_actor();
        let scope = crate::host_scope(
            command_id.as_uuid(),
            floe_execution::Cancellation::new(),
            std::time::Duration::from_secs(35),
        );
        self.execute_owner(async move {
            match command {
                ExpertCommand::SetInstallationEnabled {
                    installation_ref,
                    expected_revision,
                    enabled,
                } => owners
                    .experts
                    .set_installation_enabled(
                        &actor,
                        command_id,
                        installation_ref,
                        expected_revision,
                        enabled,
                        &scope,
                    )
                    .await
                    .map(ExpertCommandResult::Directory),
                ExpertCommand::PrepareBindingReview {
                    assignment_ref,
                    requirement_ref,
                    expected_binding_revision,
                } => owners
                    .experts
                    .prepare_binding_review(
                        &actor,
                        command_id,
                        assignment_ref,
                        requirement_ref,
                        expected_binding_revision,
                        &scope,
                    )
                    .await
                    .map(ExpertCommandResult::BindingReview),
                ExpertCommand::ReplaceBinding {
                    review_ref,
                    expected_binding_revision,
                    candidate_refs,
                } => owners
                    .experts
                    .replace_binding(
                        &actor,
                        command_id,
                        review_ref,
                        expected_binding_revision,
                        candidate_refs,
                        &scope,
                    )
                    .await
                    .map(ExpertCommandResult::Directory),
            }
        })
    }
}
impl ExpertQueries for AppComposition {
    fn expert_query(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
        query: ExpertQuery,
    ) -> Result<ExpertQueryResult, AgentFailure> {
        if request_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let owners = self.ready_owners(caller)?;
        let actor = caller.owner_actor();
        let scope = crate::host_scope(
            request_id,
            floe_execution::Cancellation::new(),
            std::time::Duration::from_secs(35),
        );
        self.execute_owner(async move {
            match query {
                ExpertQuery::Directory => owners
                    .experts
                    .directory(&actor, &scope)
                    .await
                    .map(ExpertQueryResult::Directory),
                ExpertQuery::InspectBinding {
                    assignment_ref,
                    requirement_ref,
                } => owners
                    .experts
                    .inspect_binding(&actor, assignment_ref, requirement_ref, &scope)
                    .await
                    .map(ExpertQueryResult::Binding),
                ExpertQuery::InspectBindingReview { review_ref } => owners
                    .experts
                    .inspect_binding_review(&actor, review_ref, &scope)
                    .await
                    .map(ExpertQueryResult::BindingReview),
            }
        })
    }
}
