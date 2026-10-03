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
                match (result.mode, result.view.as_ref()) {
                    (AttentionAcquisitionMode::ReadProjection, Some(view)) => {
                        let view: floe_context::AttentionView =
                            serde_json::from_value(view.clone())
                                .map_err(|_| AgentFailure::InvalidInput)?;
                        floe_context::validate_attention_view(
                            &view,
                            chrono::Utc::now().timestamp_millis(),
                        )?;
                    }
                    (AttentionAcquisitionMode::InspectSubject, None) => {}
                    _ => return Err(AgentFailure::InvalidInput),
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
                if result.mode != PersonalAcquisitionMode::ReadProjection
                    && (result.view.is_some() || result.transform_operation_id.is_some())
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                if result.mode != PersonalAcquisitionMode::InspectCatalog
                    && (!result.resources.is_empty() || result.catalog_complete)
                {
                    return Err(AgentFailure::InvalidInput);
                }
                if result.mode == PersonalAcquisitionMode::ReadProjection && result.view.is_none() {
                    return Err(AgentFailure::InvalidInput);
                }
                if let Some(value) = &result.view {
                    match result.domain {
                        PersonalDomain::People => {
                            if result.transform_operation_id.is_some() {
                                return Err(AgentFailure::PolicyDenied);
                            }
                            let view: floe_context::PeopleView =
                                serde_json::from_value(value.clone())
                                    .map_err(|_| AgentFailure::InvalidInput)?;
                            floe_context::validate_people_view(
                                &view,
                                chrono::Utc::now().timestamp_millis(),
                            )?;
                        }
                        PersonalDomain::Wellbeing => {
                            if result.transform_operation_id.is_none_or(|id| id.is_nil()) {
                                return Err(AgentFailure::PolicyDenied);
                            }
                            let view: floe_context::WellbeingView =
                                serde_json::from_value(value.clone())
                                    .map_err(|_| AgentFailure::InvalidInput)?;
                            floe_context::validate_wellbeing_view(
                                &view,
                                chrono::Utc::now().timestamp_millis(),
                            )?;
                        }
                    }
                } else if result.transform_operation_id.is_some() {
                    return Err(AgentFailure::PolicyDenied);
                }
                // The provider subsequently consumes the exact native ABI transform
                // receipt before returning any Health view to Context.
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
