use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use floe_agent::{
    AgentFailure, AttentionView, FeasibilityView, PeopleView, WellbeingView,
    validate_attention_view, validate_feasibility_view, validate_people_view,
    validate_wellbeing_view,
};
use floe_domain::{CalendarBatch, CalendarProvider, CalendarRecord, EventSchedule, PersonId};
use floe_protocol::{LocalContextOperationDto, LocalContextResultDto};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{BridgeResult, agent_failure, invalid};

const ALLOWED_VIEW_IDS: [&str; 5] = [
    "people.identity",
    "schedule.feasibility",
    "attention.coarse",
    "wellbeing.derived",
    "calendar.timeline",
];

#[derive(Clone)]
struct Entry {
    device_id: String,
    observed_at_unix_ms: i64,
    expires_at_unix_ms: i64,
    view: Value,
}

#[derive(Default)]
pub(crate) struct LocalContextStore {
    entries: Mutex<HashMap<(PersonId, String, String), Entry>>,
    calendar_observations: Mutex<HashMap<(PersonId, String), PublishedCalendarObservation>>,
}

#[derive(Clone)]
pub(crate) struct PublishedCalendarObservation {
    pub(crate) device_id: String,
    pub(crate) connection_revision: u64,
    pub(crate) provider: CalendarProvider,
    pub(crate) calendar_ids: Vec<String>,
    pub(crate) observed_at_unix_ms: i64,
    pub(crate) expires_at_unix_ms: i64,
    pub(crate) range_start_unix_ms: i64,
    pub(crate) range_end_unix_ms: i64,
    pub(crate) batches: Vec<CalendarBatch>,
}

impl LocalContextStore {
    pub(crate) fn is_available(&self, person_id: PersonId, view_id: &str) -> bool {
        self.read_entry(person_id, view_id, None).is_ok()
    }

    pub(crate) fn request(
        &self,
        person_id: PersonId,
        operation: LocalContextOperationDto,
    ) -> BridgeResult<LocalContextResultDto> {
        match operation {
            LocalContextOperationDto::Publish { device_id, view } => {
                validate_handle(&device_id, "operation.device_id")?;
                let view_id = view
                    .get("view_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("operation.view.view_id", "must be a string"))?
                    .to_owned();
                let (observed_at_unix_ms, expires_at_unix_ms) =
                    validate_view(&view_id, &view, now_unix_ms()?).map_err(agent_failure)?;
                let entry = Entry {
                    device_id: device_id.clone(),
                    observed_at_unix_ms,
                    expires_at_unix_ms,
                    view,
                };
                self.entries
                    .lock()
                    .map_err(|_| agent_failure(AgentFailure::Interrupted))?
                    .insert((person_id, device_id.clone(), view_id.clone()), entry);
                Ok(result(person_id, Some(device_id), Some(view_id), 0, None))
            }
            LocalContextOperationDto::PublishCalendarObservation {
                device_id,
                connection_revision,
                provider,
                calendar_ids,
                observed_at_unix_ms,
                expires_at_unix_ms,
                range_start_unix_ms,
                range_end_unix_ms,
                batches,
            } => {
                validate_handle(&device_id, "operation.device_id")?;
                let batches = batches
                    .into_iter()
                    .map(|batch| {
                        Ok(CalendarBatch {
                            calendar_id: batch.calendar_id,
                            records: batch
                                .records
                                .into_iter()
                                .map(|record| {
                                    Ok(CalendarRecord {
                                        can_modify: record.can_modify,
                                        calendar_id: record.calendar_id,
                                        external_id: record.external_id,
                                        external_revision: record.external_revision,
                                        title: record.title,
                                        schedule: EventSchedule::try_from(record.schedule)
                                            .map_err(|_| AgentFailure::InvalidInput)?,
                                    })
                                })
                                .collect::<Result<Vec<_>, AgentFailure>>()?,
                            failure: batch.failure,
                        })
                    })
                    .collect::<Result<Vec<_>, AgentFailure>>()
                    .map_err(agent_failure)?;
                let observation = PublishedCalendarObservation {
                    device_id: device_id.clone(),
                    connection_revision,
                    provider,
                    calendar_ids,
                    observed_at_unix_ms,
                    expires_at_unix_ms,
                    range_start_unix_ms,
                    range_end_unix_ms,
                    batches,
                };
                validate_calendar_observation(&observation, now_unix_ms()?)
                    .map_err(agent_failure)?;
                self.calendar_observations
                    .lock()
                    .map_err(|_| agent_failure(AgentFailure::Interrupted))?
                    .insert((person_id, device_id.clone()), observation);
                Ok(result(
                    person_id,
                    Some(device_id),
                    Some("calendar.timeline".into()),
                    0,
                    None,
                ))
            }
            LocalContextOperationDto::Read { view_id, device_id } => {
                validate_view_id(&view_id)?;
                if let Some(device_id) = device_id.as_deref() {
                    validate_handle(device_id, "operation.device_id")?;
                }
                let entry = self
                    .read_entry(person_id, &view_id, device_id.as_deref())
                    .map_err(agent_failure)?;
                Ok(result(
                    person_id,
                    Some(entry.device_id),
                    Some(view_id),
                    0,
                    Some(entry.view),
                ))
            }
            LocalContextOperationDto::Revoke { device_id, view_id } => {
                validate_handle(&device_id, "operation.device_id")?;
                if let Some(view_id) = view_id.as_deref() {
                    validate_view_id(view_id)?;
                }
                let mut entries = self
                    .entries
                    .lock()
                    .map_err(|_| agent_failure(AgentFailure::Interrupted))?;
                let before = entries.len();
                entries.retain(|(person, device, id), _| {
                    *person != person_id
                        || device != &device_id
                        || view_id.as_ref().is_some_and(|view_id| view_id != id)
                });
                let mut removed_count = before - entries.len();
                drop(entries);
                if view_id
                    .as_deref()
                    .is_none_or(|id| id == "calendar.timeline")
                {
                    removed_count += self
                        .calendar_observations
                        .lock()
                        .map_err(|_| agent_failure(AgentFailure::Interrupted))?
                        .remove(&(person_id, device_id.clone()))
                        .is_some() as usize;
                }
                Ok(result(
                    person_id,
                    Some(device_id),
                    view_id,
                    removed_count,
                    None,
                ))
            }
        }
    }

    pub(crate) fn people(&self, person_id: PersonId) -> Result<PeopleView, AgentFailure> {
        self.read_typed(person_id, "people.identity")
    }

    pub(crate) fn feasibility(&self, person_id: PersonId) -> Result<FeasibilityView, AgentFailure> {
        self.read_typed(person_id, "schedule.feasibility")
    }

    pub(crate) fn attention(&self, person_id: PersonId) -> Result<AttentionView, AgentFailure> {
        self.read_typed(person_id, "attention.coarse")
    }

    pub(crate) fn wellbeing(&self, person_id: PersonId) -> Result<WellbeingView, AgentFailure> {
        self.read_typed(person_id, "wellbeing.derived")
    }

    pub(crate) fn calendar_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        provider: CalendarProvider,
        calendar_ids: &[String],
        connection_revision: u64,
    ) -> Result<PublishedCalendarObservation, AgentFailure> {
        let now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        let mut observations = self
            .calendar_observations
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        observations.retain(|_, observation| observation.expires_at_unix_ms > now);
        let mut expected_ids = calendar_ids.to_vec();
        expected_ids.sort();
        observations
            .iter()
            .filter(|((person, device), observation)| {
                let mut actual_ids = observation.calendar_ids.clone();
                actual_ids.sort();
                *person == person_id
                    && device == device_id
                    && observation.provider == provider
                    && observation.connection_revision == connection_revision
                    && actual_ids == expected_ids
            })
            .map(|(_, observation)| observation)
            .max_by_key(|observation| observation.observed_at_unix_ms)
            .cloned()
            .ok_or(AgentFailure::CapabilityUnavailable)
    }

    fn read_typed<T: DeserializeOwned>(
        &self,
        person_id: PersonId,
        view_id: &str,
    ) -> Result<T, AgentFailure> {
        serde_json::from_value(self.read_entry(person_id, view_id, None)?.view)
            .map_err(|_| AgentFailure::InvalidInput)
    }

    fn read_entry(
        &self,
        person_id: PersonId,
        view_id: &str,
        device_id: Option<&str>,
    ) -> Result<Entry, AgentFailure> {
        let now = now_unix_ms().map_err(|_| AgentFailure::StaleContext)?;
        let mut entries = self.entries.lock().map_err(|_| AgentFailure::Interrupted)?;
        entries.retain(|_, entry| entry.expires_at_unix_ms > now);
        entries
            .iter()
            .filter(|((person, device, id), _)| {
                *person == person_id
                    && id == view_id
                    && device_id.is_none_or(|expected| expected == device)
            })
            .map(|(_, entry)| entry)
            .max_by_key(|entry| entry.observed_at_unix_ms)
            .cloned()
            .ok_or(AgentFailure::CapabilityUnavailable)
    }
}

fn validate_view(
    view_id: &str,
    value: &Value,
    now_unix_ms: i64,
) -> Result<(i64, i64), AgentFailure> {
    macro_rules! parse {
        ($type:ty, $validate:path) => {{
            let view: $type =
                serde_json::from_value(value.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            $validate(&view, now_unix_ms)?;
            (view.observed_at_unix_ms, view.expires_at_unix_ms)
        }};
    }
    Ok(match view_id {
        "people.identity" => parse!(PeopleView, validate_people_view),
        "schedule.feasibility" => parse!(FeasibilityView, validate_feasibility_view),
        "attention.coarse" => parse!(AttentionView, validate_attention_view),
        "wellbeing.derived" => parse!(WellbeingView, validate_wellbeing_view),
        _ => return Err(AgentFailure::CapabilityDenied),
    })
}

fn validate_view_id(view_id: &str) -> BridgeResult<()> {
    if ALLOWED_VIEW_IDS.contains(&view_id) {
        Ok(())
    } else {
        Err(invalid(
            "operation.view_id",
            "unsupported local context View",
        ))
    }
}

fn validate_calendar_observation(
    observation: &PublishedCalendarObservation,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    let identifiers: std::collections::HashSet<_> = observation.calendar_ids.iter().collect();
    let batch_ids: std::collections::HashSet<_> = observation
        .batches
        .iter()
        .map(|batch| batch.calendar_id.as_str())
        .collect();
    if !matches!(
        observation.provider,
        CalendarProvider::EventKit | CalendarProvider::Android
    ) || observation.connection_revision == 0
        || observation.calendar_ids.is_empty()
        || observation.calendar_ids.len() > 4
        || identifiers.len() != observation.calendar_ids.len()
        || observation
            .calendar_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 512)
        || observation.batches.len() != observation.calendar_ids.len()
        || batch_ids.len() != observation.batches.len()
        || !observation
            .calendar_ids
            .iter()
            .all(|id| batch_ids.contains(id.as_str()))
        || observation.observed_at_unix_ms > now_unix_ms
        || observation.expires_at_unix_ms <= now_unix_ms
        || observation.expires_at_unix_ms <= observation.observed_at_unix_ms
        || observation.expires_at_unix_ms - observation.observed_at_unix_ms > 300_000
        || observation.range_start_unix_ms < 0
        || observation.range_end_unix_ms <= observation.range_start_unix_ms
        || observation.range_end_unix_ms - observation.range_start_unix_ms > 32 * 86_400_000
    {
        return Err(AgentFailure::InvalidInput);
    }
    for batch in &observation.batches {
        if batch.records.len() > 10_000
            || batch.records.iter().any(|record| {
                record.calendar_id != batch.calendar_id
                    || record.external_id.trim().is_empty()
                    || record.external_id.len() > 512
                    || record.external_revision.trim().is_empty()
                    || record.external_revision.len() > 512
                    || record.title.len() > 4096
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(())
}

fn validate_handle(value: &str, field: &'static str) -> BridgeResult<()> {
    if !value.trim().is_empty() && value.len() <= 128 {
        Ok(())
    } else {
        Err(invalid(field, "must contain 1 to 128 characters"))
    }
}

fn now_unix_ms() -> BridgeResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| agent_failure(AgentFailure::StaleContext))?;
    i64::try_from(duration.as_millis()).map_err(|_| agent_failure(AgentFailure::StaleContext))
}

fn result(
    person_id: PersonId,
    device_id: Option<String>,
    view_id: Option<String>,
    removed_count: usize,
    view: Option<Value>,
) -> LocalContextResultDto {
    LocalContextResultDto {
        person_id: person_id.0.to_string(),
        device_id,
        view_id,
        removed_count,
        view,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_protocol::{CalendarBatchDto, CalendarRecordDto, EventScheduleDto, PROTOCOL_VERSION};
    use serde_json::json;
    use uuid::Uuid;

    fn person() -> PersonId {
        PersonId(Uuid::new_v4())
    }

    fn attention(now: i64, source: &str) -> Value {
        json!({
            "schema_version": PROTOCOL_VERSION,
            "view_id": "attention.coarse",
            "source_handle": source,
            "observed_at_unix_ms": now - 1,
            "expires_at_unix_ms": now + 60_000,
            "state": "focused",
            "confidence_millis": 800,
            "evidence_handles": ["activity:coarse"]
        })
    }

    fn calendar_publication(
        now: i64,
        device_id: &str,
        provider: CalendarProvider,
    ) -> LocalContextOperationDto {
        LocalContextOperationDto::PublishCalendarObservation {
            device_id: device_id.into(),
            connection_revision: 7,
            provider,
            calendar_ids: vec!["primary".into()],
            observed_at_unix_ms: now - 1,
            expires_at_unix_ms: now + 299_999,
            range_start_unix_ms: now - 60_000,
            range_end_unix_ms: now + 60_000,
            batches: vec![CalendarBatchDto {
                calendar_id: "primary".into(),
                records: vec![CalendarRecordDto {
                    can_modify: false,
                    calendar_id: "primary".into(),
                    external_id: "event-1".into(),
                    external_revision: "revision-1".into(),
                    title: "Review".into(),
                    schedule: EventScheduleDto::Timed {
                        starts_at: chrono::DateTime::from_timestamp_millis(now + 1_000)
                            .unwrap()
                            .to_rfc3339(),
                        ends_at: chrono::DateTime::from_timestamp_millis(now + 2_000)
                            .unwrap()
                            .to_rfc3339(),
                        timezone: "UTC".into(),
                    },
                }],
                failure: None,
            }],
        }
    }

    #[test]
    fn calendar_observation_is_person_device_provider_scope_and_revision_bound() {
        let store = LocalContextStore::default();
        let owner = person();
        let other = person();
        let now = now_unix_ms().unwrap();
        store
            .request(
                owner,
                calendar_publication(now, "iphone", CalendarProvider::EventKit),
            )
            .unwrap();
        store
            .request(
                owner,
                calendar_publication(now + 1, "ipad", CalendarProvider::EventKit),
            )
            .unwrap();

        let observation = store
            .calendar_observation(
                owner,
                "iphone",
                CalendarProvider::EventKit,
                &["primary".into()],
                7,
            )
            .unwrap();
        assert_eq!(observation.device_id, "iphone");
        assert_eq!(observation.batches[0].records[0].external_id, "event-1");
        assert!(matches!(
            store.calendar_observation(
                other,
                "iphone",
                CalendarProvider::EventKit,
                &["primary".into()],
                7,
            ),
            Err(AgentFailure::CapabilityUnavailable)
        ));
        assert!(matches!(
            store.calendar_observation(
                owner,
                "iphone",
                CalendarProvider::Android,
                &["primary".into()],
                7,
            ),
            Err(AgentFailure::CapabilityUnavailable)
        ));
        assert!(matches!(
            store.calendar_observation(
                owner,
                "iphone",
                CalendarProvider::EventKit,
                &["primary".into()],
                8,
            ),
            Err(AgentFailure::CapabilityUnavailable)
        ));
        assert!(matches!(
            store.calendar_observation(
                owner,
                "mac",
                CalendarProvider::EventKit,
                &["primary".into()],
                7,
            ),
            Err(AgentFailure::CapabilityUnavailable)
        ));
    }

    #[test]
    fn server_calendar_observation_cannot_be_published_as_device_context() {
        let store = LocalContextStore::default();
        let error = store
            .request(
                person(),
                calendar_publication(now_unix_ms().unwrap(), "mac", CalendarProvider::Google),
            )
            .unwrap_err();
        assert_eq!(error.code, floe_protocol::ErrorCodeDto::Validation);
        assert_eq!(
            error.metadata.get("agent_failure").map(String::as_str),
            Some("invalid_input")
        );
    }

    #[test]
    fn calendar_publication_wire_contract_is_explicit_and_rejects_unknown_fields() {
        let operation =
            calendar_publication(now_unix_ms().unwrap(), "iphone", CalendarProvider::EventKit);
        let value = serde_json::to_value(&operation).unwrap();
        assert_eq!(value["kind"], "publish_calendar_observation");
        assert_eq!(value["connection_revision"], 7);
        assert_eq!(value["provider"], "event_kit");
        assert_eq!(value["batches"][0]["records"][0]["external_id"], "event-1");
        let mut unknown = value;
        unknown["raw_events"] = json!([]);
        assert!(serde_json::from_value::<LocalContextOperationDto>(unknown).is_err());
    }

    #[test]
    fn publication_is_person_bound_and_latest_device_wins() {
        let store = LocalContextStore::default();
        let owner = person();
        let other = person();
        let now = now_unix_ms().unwrap();
        store
            .request(
                owner,
                LocalContextOperationDto::Publish {
                    device_id: "mac".into(),
                    view: attention(now - 10, "attention:mac"),
                },
            )
            .unwrap();
        store
            .request(
                owner,
                LocalContextOperationDto::Publish {
                    device_id: "ipad".into(),
                    view: attention(now, "attention:ipad"),
                },
            )
            .unwrap();

        assert_eq!(
            store.attention(owner).unwrap().source_handle,
            "attention:ipad"
        );
        assert_eq!(
            store.attention(other),
            Err(AgentFailure::CapabilityUnavailable)
        );
    }

    #[test]
    fn rejects_unknown_fields_and_raw_payloads() {
        let store = LocalContextStore::default();
        let now = now_unix_ms().unwrap();
        let mut view = attention(now, "attention:mac");
        view.as_object_mut()
            .unwrap()
            .insert("raw_app_history".into(), json!(["mail", "browser"]));
        assert!(
            store
                .request(
                    person(),
                    LocalContextOperationDto::Publish {
                        device_id: "mac".into(),
                        view,
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn revoke_is_device_scoped() {
        let store = LocalContextStore::default();
        let owner = person();
        let now = now_unix_ms().unwrap();
        for device in ["mac", "ipad"] {
            store
                .request(
                    owner,
                    LocalContextOperationDto::Publish {
                        device_id: device.into(),
                        view: attention(now, &format!("attention:{device}")),
                    },
                )
                .unwrap();
        }
        let result = store
            .request(
                owner,
                LocalContextOperationDto::Revoke {
                    device_id: "ipad".into(),
                    view_id: None,
                },
            )
            .unwrap();
        assert_eq!(result.removed_count, 1);
        assert_eq!(
            store.attention(owner).unwrap().source_handle,
            "attention:mac"
        );
    }
}
