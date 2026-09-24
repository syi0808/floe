use uuid::Uuid;

use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CalendarProvider, CalendarScope, CalendarSubjectPreview,
    CallerContext, ContactsAccessChange, PersonId, PersonalAccessChange, PersonalAccessOverview,
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
    Calendar {
        change: CalendarAccessChange,
    },
}

/// A native Calendar Observe change.
///
/// The review echoes what the Person saw: the connection, the reviewed
/// selection, the reviewed source authority and native subject, and the
/// reviewed grant expectation. Consumers, purpose, processing and scope stay
/// backend-derived; the wire carries none of them. Inspect is query-only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CalendarAccessChange {
    Inspect,
    Review {
        connection_id: String,
        calendar_ids: Vec<String>,
        expected_source_authority: floe_context_contract::SourceAuthority,
        expected_native_subject_fingerprint: String,
        expected_grant_id: Option<floe_access::GrantId>,
        expected_grant_authority: Option<floe_access::GrantAuthority>,
    },
    Pause {
        grant_id: floe_access::GrantId,
        expected_grant_authority: floe_access::GrantAuthority,
    },
    Remove {
        grant_id: floe_access::GrantId,
        expected_grant_authority: floe_access::GrantAuthority,
    },
}

/// A native Calendar Observe command: the change plus the device it runs on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarAccessConfiguration {
    pub device_id: String,
    pub change: CalendarAccessChange,
}

/// Native Calendar Observe state: the current selection plus the current
/// grant, if the Person granted one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarAccessOverview {
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub connection_id: String,
    pub selected_resources: Vec<String>,
    pub granted_resources: Vec<String>,
    pub source_authority: SourceAuthority,
    pub grant_id: Option<floe_access::GrantId>,
    pub grant_authority: Option<floe_access::GrantAuthority>,
    pub consumer_policy: Option<floe_context_contract::ConsumerPolicyAuthority>,
    pub state: CalendarAccessState,
    pub review_required: bool,
}

/// Native Calendar Observe lifecycle: no grant, or the current grant state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalendarAccessState {
    NeedsReview,
    Paused,
    Active,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalAccessInspection {
    CalendarSubject(CalendarSubjectIntent),
    CalendarAccess,
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
    pub calendar_access: Option<CalendarAccessOverview>,
    pub connection_observe: Option<crate::ConnectionObserveOverview>,
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
        let connection_observe = result
            .calendar_access
            .clone()
            .map(crate::ConnectionObserveOverview::from_calendar);
        Ok(LocalAccessResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            calendar_subject_preview: result.calendar_subject_preview,
            calendar_access: result.calendar_access,
            connection_observe,
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
                    consumers: crate::first_party_observe::native_consumers(connector)
                        .unwrap_or_default(),
                    change: change.clone(),
                }),
            },
            Self::Contacts { connector, change } => WorkerAction::ContactsAccess {
                change: Box::new(crate::ContactsAccessConfiguration {
                    connector: connector.clone(),
                    device_id: caller.device_id().into(),
                    consumers: crate::first_party_observe::native_consumers(connector)
                        .unwrap_or_default(),
                    change: change.clone(),
                }),
            },
            Self::Calendar { change } => WorkerAction::CalendarAccess {
                change: Box::new(CalendarAccessConfiguration {
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
            Self::CalendarAccess => LocalAccessCommand::Calendar {
                change: CalendarAccessChange::Inspect,
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
