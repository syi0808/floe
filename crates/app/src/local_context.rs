//! The device-local context this process holds open.
//!
//! Composition only: the acquisition queues belong to `floe-native`, the
//! observations they produce belong to Context, and the wire that reaches them
//! belongs to the binding. What lives here is the wiring between the three, and
//! the ordering a host lifetime change implies — a new attention host cannot
//! vouch for what the previous one observed, so its projections go with it.

use floe_agent_contract::AgentFailure;
use floe_context::{ObservationRegistry, PublishedCalendarObservation};
use floe_kernel::PersonId;
use floe_provider_adapters::sources::native_acquisition::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    AttentionBroker, CalendarAcquisitionRequest, CalendarAcquisitionResult, CalendarBroker,
    CalendarSourceFailure, HostRegistration, LocalAcquisitionBrokers, PersonalAcquisitionRequest,
    PersonalAcquisitionResult, PersonalBroker, PersonalDomain, calendar_failure,
};
use serde_json::Value;
use uuid::Uuid;

/// One device-local context command, already parsed off whatever wire brought it.
pub enum LocalContextCommand {
    RegisterAcquisitionHost {
        host_epoch: String,
    },
    PollAcquisitions {
        host_epoch: String,
    },
    CompleteAcquisition {
        host_epoch: String,
        result: Box<CalendarAcquisitionResult>,
    },
    FailAcquisition {
        host_epoch: String,
        request_id: Uuid,
        failure: CalendarSourceFailure,
    },
    DisposeAcquisitionHost {
        host_epoch: String,
    },
    RegisterAttentionHost {
        host_epoch: String,
    },
    PollAttentionAcquisitions {
        host_epoch: String,
    },
    CompleteAttentionAcquisition {
        host_epoch: String,
        result: Box<AttentionAcquisitionResult>,
    },
    FailAttentionAcquisition {
        host_epoch: String,
        request_id: Uuid,
        failure: AgentFailure,
    },
    DisposeAttentionHost {
        host_epoch: String,
    },
    RegisterPersonalHost {
        host_epoch: String,
    },
    PollPersonalAcquisitions {
        host_epoch: String,
    },
    CompletePersonalAcquisition {
        host_epoch: String,
        result: Box<PersonalAcquisitionResult>,
    },
    FailPersonalAcquisition {
        host_epoch: String,
        request_id: Uuid,
        failure: AgentFailure,
    },
    DisposePersonalHost {
        host_epoch: String,
    },
    Publish {
        device_id: String,
        view_id: String,
        view: Value,
    },
    PublishCalendarObservation {
        device_id: String,
        observation: Box<CalendarObservationPublication>,
    },
    Read {
        view_id: String,
        device_id: Option<String>,
    },
    Revoke {
        device_id: String,
        view_id: Option<String>,
    },
}

/// One calendar observation a device published.
///
/// The source authority is not on the wire: it is read from the Person's live
/// connection, so a device cannot claim an authority it was not granted.
pub struct CalendarObservationPublication {
    pub connection_id: String,
    pub connection_revision: u64,
    pub provider: floe_context_contract::CalendarProvider,
    pub calendar_ids: Vec<String>,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub batches: Vec<floe_day::CalendarBatch>,
}

/// What one command produced.
#[derive(Default)]
pub struct LocalContextOutcome {
    pub device_id: Option<String>,
    pub view_id: Option<String>,
    pub removed_count: usize,
    pub view: Option<Value>,
    pub acquisitions: Vec<CalendarAcquisitionRequest>,
    pub attention_acquisitions: Vec<AttentionAcquisitionRequest>,
    pub personal_acquisitions: Vec<PersonalAcquisitionRequest>,
}

/// The acquisition queues and the observations this process stands behind.
pub struct LocalContextHost {
    brokers: LocalAcquisitionBrokers,
    observations: ObservationRegistry,
}

impl Default for LocalContextHost {
    fn default() -> Self {
        Self {
            brokers: LocalAcquisitionBrokers::new(),
            observations: ObservationRegistry::new(),
        }
    }
}

impl LocalContextHost {
    pub fn calendar(&self) -> &CalendarBroker {
        self.brokers.calendar()
    }

    pub fn attention(&self) -> &AttentionBroker {
        self.brokers.attention()
    }

    pub fn personal(&self) -> &PersonalBroker {
        self.brokers.personal()
    }

    pub fn observations(&self) -> &ObservationRegistry {
        &self.observations
    }

    pub fn process_incarnation(&self) -> Uuid {
        self.observations.process_incarnation()
    }

    /// Run one command against this Person's device context.
    ///
    /// `connection` is the Person's current calendar connection, which only a
    /// calendar publication needs; the caller reads it before entering here so
    /// that no storage is reached from inside.
    pub fn execute(
        &self,
        person_id: PersonId,
        command: LocalContextCommand,
        connection: Option<&floe_day::CalendarConnection>,
    ) -> Result<LocalContextOutcome, AgentFailure> {
        let now_unix_ms = chrono::Utc::now().timestamp_millis();
        match command {
            LocalContextCommand::RegisterAcquisitionHost { host_epoch } => {
                self.calendar().register_host(person_id, host_epoch)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::PollAcquisitions { host_epoch } => Ok(LocalContextOutcome {
                acquisitions: self.calendar().poll(person_id, &host_epoch)?,
                ..LocalContextOutcome::default()
            }),
            LocalContextCommand::CompleteAcquisition { host_epoch, result } => {
                self.calendar().complete(person_id, &host_epoch, *result)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::FailAcquisition {
                host_epoch,
                request_id,
                failure,
            } => {
                self.calendar().fail(
                    person_id,
                    &host_epoch,
                    request_id,
                    calendar_failure(failure),
                )?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::DisposeAcquisitionHost { host_epoch } => {
                self.calendar().dispose_host(person_id, &host_epoch)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::RegisterAttentionHost { host_epoch } => {
                // A new host cannot vouch for the previous one's projections.
                if self.attention().register_host(person_id, host_epoch)?
                    == HostRegistration::Replaced
                {
                    self.observations.invalidate_trusted_attention(person_id)?;
                }
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::PollAttentionAcquisitions { host_epoch } => {
                Ok(LocalContextOutcome {
                    attention_acquisitions: self.attention().poll(person_id, &host_epoch)?,
                    ..LocalContextOutcome::default()
                })
            }
            LocalContextCommand::CompleteAttentionAcquisition { host_epoch, result } => {
                // A projection must carry a view Context accepts; an inspect
                // must carry none at all.
                match (result.mode, result.view.as_ref()) {
                    (AttentionAcquisitionMode::ReadProjection, Some(view)) => {
                        let view: floe_context::AttentionView =
                            serde_json::from_value(view.clone())
                                .map_err(|_| AgentFailure::InvalidInput)?;
                        floe_context::validate_attention_view(&view, now_unix_ms)?;
                    }
                    (AttentionAcquisitionMode::ReadProjection, None)
                    | (AttentionAcquisitionMode::InspectSubject, Some(_)) => {
                        return Err(AgentFailure::InvalidInput);
                    }
                    (AttentionAcquisitionMode::InspectSubject, None) => {}
                }
                self.attention().complete(person_id, &host_epoch, *result)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::FailAttentionAcquisition {
                host_epoch,
                request_id,
                failure,
            } => {
                self.attention()
                    .fail(person_id, &host_epoch, request_id, failure)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::DisposeAttentionHost { host_epoch } => {
                self.attention().dispose_host(person_id, &host_epoch)?;
                self.observations.invalidate_trusted_attention(person_id)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::RegisterPersonalHost { host_epoch } => {
                self.personal().register_host(person_id, host_epoch)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::PollPersonalAcquisitions { host_epoch } => {
                Ok(LocalContextOutcome {
                    personal_acquisitions: self.personal().poll(person_id, &host_epoch)?,
                    ..LocalContextOutcome::default()
                })
            }
            LocalContextCommand::CompletePersonalAcquisition { host_epoch, result } => {
                // A personal read is only admissible if it returns the view its
                // domain promised.
                let view = result.view.as_ref().ok_or(AgentFailure::InvalidInput)?;
                let view_id = match result.domain {
                    PersonalDomain::People => "people.identity",
                    PersonalDomain::Wellbeing => "wellbeing.derived",
                    PersonalDomain::Feasibility => "schedule.feasibility",
                };
                floe_context::validate_view(view_id, view, now_unix_ms)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                self.personal().complete(person_id, &host_epoch, *result)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::FailPersonalAcquisition {
                host_epoch,
                request_id,
                failure,
            } => {
                self.personal()
                    .fail(person_id, &host_epoch, request_id, failure)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::DisposePersonalHost { host_epoch } => {
                self.personal().dispose_host(person_id, &host_epoch)?;
                Ok(LocalContextOutcome::default())
            }
            LocalContextCommand::Publish {
                device_id,
                view_id,
                view,
            } => {
                self.observations
                    .publish(person_id, &device_id, &view_id, view, now_unix_ms)?;
                Ok(LocalContextOutcome {
                    device_id: Some(device_id),
                    view_id: Some(view_id),
                    ..LocalContextOutcome::default()
                })
            }
            LocalContextCommand::PublishCalendarObservation {
                device_id,
                observation,
            } => {
                let connection = connection.ok_or(AgentFailure::CapabilityUnavailable)?;
                let observation = *observation;
                self.observations.publish_calendar_observation(
                    person_id,
                    &device_id,
                    PublishedCalendarObservation {
                        connection_id: observation.connection_id,
                        source_authority: connection.source_authority,
                        connection_revision: observation.connection_revision,
                        provider: observation.provider,
                        calendar_ids: observation.calendar_ids,
                        observed_at_unix_ms: observation.observed_at_unix_ms,
                        expires_at_unix_ms: observation.expires_at_unix_ms,
                        range_start_unix_ms: observation.range_start_unix_ms,
                        range_end_unix_ms: observation.range_end_unix_ms,
                        batches: observation.batches,
                    },
                    connection,
                    now_unix_ms,
                )?;
                Ok(LocalContextOutcome {
                    device_id: Some(device_id),
                    view_id: Some("calendar.timeline".into()),
                    ..LocalContextOutcome::default()
                })
            }
            LocalContextCommand::Read { view_id, device_id } => {
                let entry = self.observations.read_entry(
                    person_id,
                    &view_id,
                    device_id.as_deref(),
                    now_unix_ms,
                )?;
                Ok(LocalContextOutcome {
                    device_id: Some(entry.device_id),
                    view_id: Some(view_id),
                    view: Some(entry.view),
                    ..LocalContextOutcome::default()
                })
            }
            LocalContextCommand::Revoke { device_id, view_id } => {
                let removed_count =
                    self.observations
                        .revoke(person_id, &device_id, view_id.as_deref())?;
                Ok(LocalContextOutcome {
                    device_id: Some(device_id),
                    view_id,
                    removed_count,
                    ..LocalContextOutcome::default()
                })
            }
        }
    }
}
