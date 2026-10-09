use crate::local_context::{NativeHostKind, NativeHostOutcome, NativeHostRegistrationRef};
use crate::{
    AgentFailure, AppComposition, AttentionAcquisitionMode, AttentionAcquisitionResult,
    CalendarAcquisitionMode, CalendarAcquisitionResult, CalendarProvider, CalendarSourceFailure,
    CallerContext, NativeCalendarBatch, PersonId, PersonalAcquisitionResult, PersonalDomain,
};
use floe_provider_adapters::sources::native_acquisition::{
    NativeSourceResource, PersonalAcquisitionMode,
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
    pub available_calendars: Vec<NativeSourceResource>,
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
            available_calendars: self.available_calendars,
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
    pub mode: PersonalAcquisitionMode,
    pub native_subject_fingerprint_before: String,
    pub native_subject_fingerprint_after: String,
    pub permission_class: String,
    pub provider: String,
    pub view: Option<serde_json::Value>,
    pub transform_operation_id: Option<Uuid>,
    pub resources: Vec<NativeSourceResource>,
    pub catalog_complete: bool,
}
impl PersonalCompletion {
    fn bind(self, caller: &CallerContext) -> PersonalAcquisitionResult {
        PersonalAcquisitionResult {
            person_id: PersonId(caller.person_id()),
            device_id: caller.device_id().into(),
            request_id: self.request_id,
            host_epoch: self.host_epoch,
            domain: self.domain,
            mode: self.mode,
            native_subject_fingerprint_before: self.native_subject_fingerprint_before,
            native_subject_fingerprint_after: self.native_subject_fingerprint_after,
            permission_class: self.permission_class,
            provider: self.provider,
            view: self.view,
            transform_operation_id: self.transform_operation_id,
            resources: self.resources,
            catalog_complete: self.catalog_complete,
        }
    }
}

pub enum NativeHostCommand {
    Register {
        kind: NativeHostKind,
    },
    Dispose {
        kind: NativeHostKind,
        registration: NativeHostRegistrationRef,
    },
    CompleteCalendar {
        registration: NativeHostRegistrationRef,
        result: Box<CalendarCompletion>,
    },
    FailCalendar {
        registration: NativeHostRegistrationRef,
        request_id: Uuid,
        failure: CalendarSourceFailure,
    },
    CompleteAttention {
        registration: NativeHostRegistrationRef,
        result: Box<AttentionCompletion>,
    },
    FailAttention {
        registration: NativeHostRegistrationRef,
        request_id: Uuid,
        failure: AgentFailure,
    },
    CompletePersonal {
        registration: NativeHostRegistrationRef,
        result: Box<PersonalCompletion>,
    },
    FailPersonal {
        registration: NativeHostRegistrationRef,
        request_id: Uuid,
        failure: AgentFailure,
    },
}
pub enum NativeHostQuery {
    Poll {
        kind: NativeHostKind,
        registration: NativeHostRegistrationRef,
    },
}
pub trait NativeHostCommands {
    fn apply_native_host(
        &self,
        caller: &CallerContext,
        command: NativeHostCommand,
    ) -> Result<NativeHostOutcome, AgentFailure>;
}
pub trait NativeHostQueries {
    fn query_native_host(
        &self,
        caller: &CallerContext,
        query: NativeHostQuery,
    ) -> Result<NativeHostOutcome, AgentFailure>;
}
impl NativeHostCommands for crate::LocalContextHost {
    fn apply_native_host(
        &self,
        caller: &CallerContext,
        command: NativeHostCommand,
    ) -> Result<NativeHostOutcome, AgentFailure> {
        let host = self;
        let person = PersonId(caller.person_id());
        match command {
            NativeHostCommand::Register { kind } => {
                return Ok(NativeHostOutcome::Registered(host.register(caller, kind)?));
            }
            NativeHostCommand::Dispose { kind, registration } => {
                host.dispose(caller, kind, &registration)?
            }
            NativeHostCommand::CompleteCalendar {
                registration,
                result,
            } => {
                host.validate_registration(caller, NativeHostKind::Calendar, &registration)?;
                if result.host_epoch != registration.host_epoch {
                    return Err(AgentFailure::PolicyDenied);
                }
                host.calendar()
                    .complete(person, &registration.host_epoch, result.bind(caller))?;
            }
            NativeHostCommand::FailCalendar {
                registration,
                request_id,
                failure,
            } => {
                host.validate_registration(caller, NativeHostKind::Calendar, &registration)?;
                host.calendar().fail(
                    person,
                    &registration.host_epoch,
                    request_id,
                    floe_provider_adapters::sources::native_acquisition::calendar_failure(failure),
                )?;
            }
            NativeHostCommand::CompleteAttention {
                registration,
                result,
            } => {
                host.validate_registration(caller, NativeHostKind::Attention, &registration)?;
                if result.host_epoch != registration.host_epoch {
                    return Err(AgentFailure::PolicyDenied);
                }
                host.attention()
                    .complete(person, &registration.host_epoch, result.bind(caller))?;
            }
            NativeHostCommand::FailAttention {
                registration,
                request_id,
                failure,
            } => {
                host.validate_registration(caller, NativeHostKind::Attention, &registration)?;
                host.attention()
                    .fail(person, &registration.host_epoch, request_id, failure)?;
            }
            NativeHostCommand::CompletePersonal {
                registration,
                result,
            } => {
                host.validate_registration(caller, NativeHostKind::Personal, &registration)?;
                if result.host_epoch != registration.host_epoch {
                    return Err(AgentFailure::PolicyDenied);
                }
                host.personal()
                    .complete(person, &registration.host_epoch, result.bind(caller))?;
            }
            NativeHostCommand::FailPersonal {
                registration,
                request_id,
                failure,
            } => {
                host.validate_registration(caller, NativeHostKind::Personal, &registration)?;
                host.personal()
                    .fail(person, &registration.host_epoch, request_id, failure)?;
            }
        }
        Ok(NativeHostOutcome::Acknowledged)
    }
}
impl NativeHostQueries for crate::LocalContextHost {
    fn query_native_host(
        &self,
        caller: &CallerContext,
        query: NativeHostQuery,
    ) -> Result<NativeHostOutcome, AgentFailure> {
        let NativeHostQuery::Poll { kind, registration } = query;
        self.validate_registration(caller, kind, &registration)?;
        let person = PersonId(caller.person_id());
        Ok(match kind {
            NativeHostKind::Calendar => NativeHostOutcome::CalendarAcquisitions(
                self.calendar().poll(person, &registration.host_epoch)?,
            ),
            NativeHostKind::Attention => NativeHostOutcome::AttentionAcquisitions(
                self.attention().poll(person, &registration.host_epoch)?,
            ),
            NativeHostKind::Personal => NativeHostOutcome::PersonalAcquisitions(
                self.personal().poll(person, &registration.host_epoch)?,
            ),
        })
    }
}

impl NativeHostCommands for AppComposition {
    fn apply_native_host(
        &self,
        caller: &CallerContext,
        command: NativeHostCommand,
    ) -> Result<NativeHostOutcome, AgentFailure> {
        self.local_context.apply_native_host(caller, command)
    }
}
impl NativeHostQueries for AppComposition {
    fn query_native_host(
        &self,
        caller: &CallerContext,
        query: NativeHostQuery,
    ) -> Result<NativeHostOutcome, AgentFailure> {
        self.local_context.query_native_host(caller, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalContextHost, LocalIdentityClaim};

    fn caller(person_id: Uuid, runtime_epoch: u64) -> CallerContext {
        CallerContext::verified(
            LocalIdentityClaim {
                person_id,
                device_id: "verified-device".into(),
            },
            runtime_epoch,
        )
        .unwrap()
    }

    fn registration(
        host: &LocalContextHost,
        caller: &CallerContext,
        kind: NativeHostKind,
    ) -> NativeHostRegistrationRef {
        let NativeHostOutcome::Registered(registration) = host
            .apply_native_host(caller, NativeHostCommand::Register { kind })
            .unwrap()
        else {
            panic!("host registration must return its reference");
        };
        registration
    }

    #[test]
    fn personal_completion_keeps_caller_and_runtime_epoch_fences() {
        let host = LocalContextHost::default();
        let person_id = Uuid::new_v4();
        let caller_ctx = caller(person_id, 3);
        let registration = registration(&host, &caller_ctx, NativeHostKind::Personal);
        let completion = || NativeHostCommand::CompletePersonal {
            registration: registration.clone(),
            result: Box::new(PersonalCompletion {
                request_id: Uuid::new_v4(),
                host_epoch: registration.host_epoch.clone(),
                domain: PersonalDomain::People,
                mode: PersonalAcquisitionMode::InspectSubject,
                native_subject_fingerprint_before: "subject".into(),
                native_subject_fingerprint_after: "subject".into(),
                permission_class: "authorized".into(),
                provider: "apple".into(),
                view: None,
                transform_operation_id: None,
                resources: Vec::new(),
                catalog_complete: false,
            }),
        };

        let wrong_runtime = caller(person_id, 4);
        assert!(matches!(
            host.apply_native_host(&wrong_runtime, completion()),
            Err(AgentFailure::PolicyDenied)
        ));

        let mut stale_epoch = completion();
        let NativeHostCommand::CompletePersonal { result, .. } = &mut stale_epoch else {
            unreachable!();
        };
        result.host_epoch = "stale-host-epoch".into();
        assert!(matches!(
            host.apply_native_host(&caller_ctx, stale_epoch),
            Err(AgentFailure::PolicyDenied)
        ));
    }

    #[test]
    fn attention_completion_keeps_verified_person_and_device_fence() {
        let host = LocalContextHost::default();
        let person_id = Uuid::new_v4();
        let caller_ctx = caller(person_id, 5);
        let registration = registration(&host, &caller_ctx, NativeHostKind::Attention);
        let command = || NativeHostCommand::CompleteAttention {
            registration: registration.clone(),
            result: Box::new(AttentionCompletion {
                request_id: Uuid::new_v4(),
                host_epoch: "host-epoch".into(),
                mode: AttentionAcquisitionMode::InspectSubject,
                native_subject_fingerprint_before: "subject".into(),
                native_subject_fingerprint_after: "subject".into(),
                permission_class: "session_observation".into(),
                view: None,
            }),
        };
        let wrong_person = caller(Uuid::new_v4(), 5);
        let wrong_device = CallerContext::verified(
            LocalIdentityClaim {
                person_id,
                device_id: "different-device".into(),
            },
            5,
        )
        .unwrap();

        assert!(matches!(
            host.apply_native_host(&wrong_person, command()),
            Err(AgentFailure::PolicyDenied)
        ));
        assert!(matches!(
            host.apply_native_host(&wrong_device, command()),
            Err(AgentFailure::PolicyDenied)
        ));
    }
}
