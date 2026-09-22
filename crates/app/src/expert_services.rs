use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CalendarProvider, CalendarScope, CallerContext, ServiceError,
    SourceAuthority, VaultState,
};

pub use floe_experts::{
    CalendarExpertOverview, RegistryConfiguration, RegistryConfigurationTarget, RegistryOverview,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarExpertInstall {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub provider: CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScope,
    pub connection_revision: u64,
    pub source_authority: Option<SourceAuthority>,
    pub reviewed_native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertCommand {
    ConfigureRegistry(RegistryConfiguration),
    InstallCalendar(CalendarExpertInstall),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpertInspection {
    Registry,
    Calendar,
}

#[derive(Clone, Debug)]
pub struct ExpertOperationResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub registry: Option<RegistryOverview>,
    pub calendar_experts: Option<CalendarExpertOverview>,
    pub failure: Option<AgentFailure>,
}

pub trait ExpertCommands {
    fn expert_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: ExpertCommand,
    ) -> Result<ExpertOperationResult, ServiceError>;
}

pub trait ExpertQueries {
    fn inspect_experts(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: ExpertInspection,
    ) -> Result<ExpertOperationResult, ServiceError>;
    fn read_expert_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ExpertOperationResult, ServiceError>;
}

impl ExpertCommands for AppComposition {
    fn expert_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: ExpertCommand,
    ) -> Result<ExpertOperationResult, ServiceError> {
        self.expert_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::ExpertCommand(command)),
            false,
        )
    }
}

impl ExpertQueries for AppComposition {
    fn inspect_experts(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: ExpertInspection,
    ) -> Result<ExpertOperationResult, ServiceError> {
        self.expert_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::ExpertInspection(inspection)),
            false,
        )
    }
    fn read_expert_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ExpertOperationResult, ServiceError> {
        self.expert_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn expert_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<ExpertOperationResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Experts,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(ExpertOperationResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            registry: result.registry,
            calendar_experts: result.calendar_experts,
            failure: result.failure,
        })
    }
}

impl CalendarExpertInstall {
    pub(crate) fn bind(&self, caller: &CallerContext) -> floe_experts::CalendarExpertSetup {
        floe_experts::CalendarExpertSetup {
            instance_id: self.instance_id,
            expected_revision: self.expected_revision,
            setup_id: self.setup_id,
            provider: self.provider,
            device_id: caller.device_id().into(),
            calendar_ids: self.calendar_ids.clone(),
            connection_scope: self.connection_scope,
            connection_revision: self.connection_revision,
            source_authority: self.source_authority,
            reviewed_native_subject_fingerprint: self.reviewed_native_subject_fingerprint.clone(),
        }
    }
}
