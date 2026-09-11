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
use floe_domain::PersonId;
use floe_protocol::{LocalContextOperationDto, LocalContextResultDto};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{BridgeResult, agent_failure, invalid};

const ALLOWED_VIEW_IDS: [&str; 4] = [
    "people.identity",
    "schedule.feasibility",
    "attention.coarse",
    "wellbeing.derived",
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
}

impl LocalContextStore {
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
                Ok(result(
                    person_id,
                    Some(device_id),
                    view_id,
                    before - entries.len(),
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
    use floe_protocol::PROTOCOL_VERSION;
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
