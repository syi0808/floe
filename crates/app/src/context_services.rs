use crate::{
    AgentFailure, AppComposition, AttentionAcquisitionMode, AttentionAcquisitionResult,
    CalendarAcquisitionMode, CalendarAcquisitionResult, CalendarObservationPublication,
    CalendarProvider, CalendarSourceFailure, CallerContext, LocalContextCommand,
    LocalContextOutcome, NativeCalendarBatch, PersonId, PersonalAcquisitionResult, PersonalDomain,
};
use uuid::Uuid;

pub struct CalendarCompletion {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub connection_id: String,
    pub connection_revision: u64,
    pub provider: CalendarProvider,
    pub mode: CalendarAcquisitionMode,
    pub calendar_ids: Vec<String>,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub available_calendar_ids: Vec<String>,
    pub permission_class: String,
    pub batches: Vec<NativeCalendarBatch>,
}
impl CalendarCompletion {
    fn bind(self, caller: &CallerContext) -> CalendarAcquisitionResult {
        CalendarAcquisitionResult {
            person_id: PersonId(caller.person_id()),
            device_id: caller.device_id().into(),
            request_id: self.request_id,
            host_epoch: self.host_epoch,
            connection_id: self.connection_id,
            connection_revision: self.connection_revision,
            provider: self.provider,
            mode: self.mode,
            calendar_ids: self.calendar_ids,
            range_start_unix_ms: self.range_start_unix_ms,
            range_end_unix_ms: self.range_end_unix_ms,
            native_subject_fingerprint_before: self.native_subject_fingerprint_before,
            native_subject_fingerprint_after: self.native_subject_fingerprint_after,
            available_calendar_ids: self.available_calendar_ids,
            permission_class: self.permission_class,
            batches: self.batches,
        }
    }
}
pub struct AttentionCompletion {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub mode: AttentionAcquisitionMode,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub view: Option<serde_json::Value>,
}
impl AttentionCompletion {
    fn bind(self, caller: &CallerContext) -> AttentionAcquisitionResult {
        AttentionAcquisitionResult {
            person_id: PersonId(caller.person_id()),
            device_id: caller.device_id().into(),
            request_id: self.request_id,
            host_epoch: self.host_epoch,
            mode: self.mode,
            native_subject_fingerprint_before: self.native_subject_fingerprint_before,
            native_subject_fingerprint_after: self.native_subject_fingerprint_after,
            permission_class: self.permission_class,
            view: self.view,
        }
    }
}
pub struct PersonalCompletion {
    pub request_id: Uuid,
    pub host_epoch: String,
    pub domain: PersonalDomain,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub provider: String,
    pub view: Option<serde_json::Value>,
}
impl PersonalCompletion {
    fn bind(self, caller: &CallerContext) -> PersonalAcquisitionResult {
        PersonalAcquisitionResult {
            person_id: PersonId(caller.person_id()),
            device_id: caller.device_id().into(),
            request_id: self.request_id,
            host_epoch: self.host_epoch,
            domain: self.domain,
            native_subject_fingerprint_before: self.native_subject_fingerprint_before,
            native_subject_fingerprint_after: self.native_subject_fingerprint_after,
            permission_class: self.permission_class,
            provider: self.provider,
            view: self.view,
        }
    }
}

pub enum ContextCommand {
    RegisterAcquisitionHost {
        host_epoch: String,
    },
    DisposeAcquisitionHost {
        host_epoch: String,
    },
    CompleteAcquisition {
        host_epoch: String,
        result: Box<CalendarCompletion>,
    },
    FailAcquisition {
        host_epoch: String,
        request_id: Uuid,
        failure: CalendarSourceFailure,
    },
    RegisterAttentionHost {
        host_epoch: String,
    },
    DisposeAttentionHost {
        host_epoch: String,
    },
    CompleteAttentionAcquisition {
        host_epoch: String,
        result: Box<AttentionCompletion>,
    },
    FailAttentionAcquisition {
        host_epoch: String,
        request_id: Uuid,
        failure: AgentFailure,
    },
    RegisterPersonalHost {
        host_epoch: String,
    },
    DisposePersonalHost {
        host_epoch: String,
    },
    CompletePersonalAcquisition {
        host_epoch: String,
        result: Box<PersonalCompletion>,
    },
    FailPersonalAcquisition {
        host_epoch: String,
        request_id: Uuid,
        failure: AgentFailure,
    },
    Publish {
        view_id: String,
        view: serde_json::Value,
    },
    PublishCalendarObservation {
        observation: Box<CalendarObservationPublication>,
    },
    Revoke {
        view_id: Option<String>,
    },
}

pub enum ContextQuery {
    PollAcquisitions { host_epoch: String },
    PollAttentionAcquisitions { host_epoch: String },
    PollPersonalAcquisitions { host_epoch: String },
    Read { view_id: String },
}

pub trait LocalContextCommands {
    fn apply_local_context(
        &self,
        caller: &CallerContext,
        command: ContextCommand,
    ) -> Result<LocalContextOutcome, AgentFailure>;
}

pub trait LocalContextQueries {
    fn query_local_context(
        &self,
        caller: &CallerContext,
        query: ContextQuery,
    ) -> Result<LocalContextOutcome, AgentFailure>;
}

impl ContextCommand {
    fn bind(self, caller: &CallerContext) -> LocalContextCommand {
        match self {
            Self::RegisterAcquisitionHost { host_epoch } => {
                LocalContextCommand::RegisterAcquisitionHost { host_epoch }
            }
            Self::DisposeAcquisitionHost { host_epoch } => {
                LocalContextCommand::DisposeAcquisitionHost { host_epoch }
            }
            Self::CompleteAcquisition { host_epoch, result } => {
                LocalContextCommand::CompleteAcquisition {
                    host_epoch,
                    result: Box::new(result.bind(caller)),
                }
            }
            Self::FailAcquisition {
                host_epoch,
                request_id,
                failure,
            } => LocalContextCommand::FailAcquisition {
                host_epoch,
                request_id,
                failure,
            },
            Self::RegisterAttentionHost { host_epoch } => {
                LocalContextCommand::RegisterAttentionHost { host_epoch }
            }
            Self::DisposeAttentionHost { host_epoch } => {
                LocalContextCommand::DisposeAttentionHost { host_epoch }
            }
            Self::CompleteAttentionAcquisition { host_epoch, result } => {
                LocalContextCommand::CompleteAttentionAcquisition {
                    host_epoch,
                    result: Box::new(result.bind(caller)),
                }
            }
            Self::FailAttentionAcquisition {
                host_epoch,
                request_id,
                failure,
            } => LocalContextCommand::FailAttentionAcquisition {
                host_epoch,
                request_id,
                failure,
            },
            Self::RegisterPersonalHost { host_epoch } => {
                LocalContextCommand::RegisterPersonalHost { host_epoch }
            }
            Self::DisposePersonalHost { host_epoch } => {
                LocalContextCommand::DisposePersonalHost { host_epoch }
            }
            Self::CompletePersonalAcquisition { host_epoch, result } => {
                LocalContextCommand::CompletePersonalAcquisition {
                    host_epoch,
                    result: Box::new(result.bind(caller)),
                }
            }
            Self::FailPersonalAcquisition {
                host_epoch,
                request_id,
                failure,
            } => LocalContextCommand::FailPersonalAcquisition {
                host_epoch,
                request_id,
                failure,
            },
            Self::Publish { view_id, view } => LocalContextCommand::Publish {
                device_id: caller.device_id().into(),
                view_id,
                view,
            },
            Self::PublishCalendarObservation { observation } => {
                LocalContextCommand::PublishCalendarObservation {
                    device_id: caller.device_id().into(),
                    observation,
                }
            }
            Self::Revoke { view_id } => LocalContextCommand::Revoke {
                device_id: caller.device_id().into(),
                view_id,
            },
        }
    }
}
impl ContextQuery {
    fn bind(self, caller: &CallerContext) -> LocalContextCommand {
        match self {
            Self::PollAcquisitions { host_epoch } => {
                LocalContextCommand::PollAcquisitions { host_epoch }
            }
            Self::PollAttentionAcquisitions { host_epoch } => {
                LocalContextCommand::PollAttentionAcquisitions { host_epoch }
            }
            Self::PollPersonalAcquisitions { host_epoch } => {
                LocalContextCommand::PollPersonalAcquisitions { host_epoch }
            }
            Self::Read { view_id } => LocalContextCommand::Read {
                device_id: Some(caller.device_id().into()),
                view_id,
            },
        }
    }
}
impl LocalContextCommands for AppComposition {
    fn apply_local_context(
        &self,
        caller: &CallerContext,
        command: ContextCommand,
    ) -> Result<LocalContextOutcome, AgentFailure> {
        let person = PersonId(caller.person_id());
        let connection = if matches!(command, ContextCommand::PublishCalendarObservation { .. }) {
            Some(
                self.runtime
                    .block_on(self.core.calendar_connection(person))
                    .map_err(|_| AgentFailure::StorageUnavailable)?
                    .ok_or(AgentFailure::CapabilityUnavailable)?,
            )
        } else {
            None
        };
        self.local_context
            .execute(person, command.bind(caller), connection.as_ref())
    }
}
impl LocalContextQueries for AppComposition {
    fn query_local_context(
        &self,
        caller: &CallerContext,
        query: ContextQuery,
    ) -> Result<LocalContextOutcome, AgentFailure> {
        self.local_context
            .execute(PersonId(caller.person_id()), query.bind(caller), None)
    }
}
