use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CalendarProvider, CalendarScope, CalendarSubjectPreview,
    CallerContext, ContactsAccessChange, PersonalAccessChange, PersonalAccessOverview,
    ServiceError, SourceAuthority, VaultState, WorkerAction,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarSubjectIntent {
    pub provider: CalendarProvider,
    pub connection_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScope,
    pub connection_revision: u64,
    pub source_authority: SourceAuthority,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LocalAccessCommand {
    Personal {
        connector: String,
        change: PersonalAccessChange,
    },
    Contacts {
        connector: String,
        change: ContactsAccessChange,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalAccessInspection {
    CalendarSubject(CalendarSubjectIntent),
    Personal {
        connector: String,
    },
    Contacts {
        connector: String,
        selected_handles: Vec<String>,
    },
}

#[derive(Clone)]
pub struct LocalAccessResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub calendar_subject_preview: Option<CalendarSubjectPreview>,
    pub personal_access: Option<PersonalAccessOverview>,
    pub failure: Option<AgentFailure>,
}

pub trait LocalAccessCommands {
    fn local_access_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: LocalAccessCommand,
    ) -> Result<LocalAccessResult, ServiceError>;
}

pub trait LocalAccessQueries {
    fn inspect_local_access(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: LocalAccessInspection,
    ) -> Result<LocalAccessResult, ServiceError>;
    fn read_local_access_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<LocalAccessResult, ServiceError>;
}

impl LocalAccessCommands for AppComposition {
    fn local_access_command(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        command: LocalAccessCommand,
    ) -> Result<LocalAccessResult, ServiceError> {
        self.local_access_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::LocalAccessCommand(command)),
            false,
        )
    }
}

impl LocalAccessQueries for AppComposition {
    fn inspect_local_access(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: LocalAccessInspection,
    ) -> Result<LocalAccessResult, ServiceError> {
        self.local_access_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::LocalAccessInspection(inspection)),
            false,
        )
    }
    fn read_local_access_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<LocalAccessResult, ServiceError> {
        self.local_access_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn local_access_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<LocalAccessResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Access,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(LocalAccessResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            calendar_subject_preview: result.calendar_subject_preview,
            personal_access: result.personal_access,
            failure: result.failure,
        })
    }
}

impl LocalAccessCommand {
    pub(crate) fn action(&self, caller: &CallerContext) -> WorkerAction {
        match self {
            Self::Personal { connector, change } => WorkerAction::PersonalAccess {
                change: Box::new(crate::PersonalAccessConfiguration {
                    connector: connector.clone(),
                    device_id: caller.device_id().into(),
                    change: change.clone(),
                }),
            },
            Self::Contacts { connector, change } => WorkerAction::ContactsAccess {
                change: Box::new(crate::ContactsAccessConfiguration {
                    connector: connector.clone(),
                    device_id: caller.device_id().into(),
                    change: change.clone(),
                }),
            },
        }
    }
}

impl LocalAccessInspection {
    pub(crate) fn action(&self, caller: &CallerContext) -> WorkerAction {
        match self {
            Self::Personal { connector } => LocalAccessCommand::Personal {
                connector: connector.clone(),
                change: PersonalAccessChange::Inspect,
            }
            .action(caller),
            Self::Contacts {
                connector,
                selected_handles,
            } => LocalAccessCommand::Contacts {
                connector: connector.clone(),
                change: ContactsAccessChange::Inspect {
                    selected_handles: selected_handles.clone(),
                },
            }
            .action(caller),
            Self::CalendarSubject(request) => WorkerAction::CalendarSubjectPreview {
                request: Box::new(crate::CalendarSubjectRequest {
                    provider: request.provider,
                    device_id: caller.device_id().into(),
                    connection_id: request.connection_id.clone(),
                    calendar_ids: request.calendar_ids.clone(),
                    connection_scope: request.connection_scope,
                    connection_revision: request.connection_revision,
                    source_authority: request.source_authority,
                }),
            },
        }
    }
}
