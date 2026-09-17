//! What one personal-source read owes its provenance to.
//!
//! A stored dependency names the observation it came from. Two reads of the
//! same source are the same read only when every input that shaped the answer
//! is the same, so the query fingerprint covers the view's own handle and
//! freshness, the device subject that answered, the observation and the process
//! that held it, and whatever the read asked for. Context owns this because
//! Context is what a later turn asks whether a dependency still holds.

use sha2::Digest;
use uuid::Uuid;

use floe_context_contract::{AttentionView, FeasibilityView, PeopleView, PersonId, WellbeingView};

fn digest(value: String) -> Vec<u8> {
    sha2::Sha256::digest(value.as_bytes()).to_vec()
}

/// The device subject an attention grant was reviewed against.
pub fn attention_subject_fingerprint(
    person_id: PersonId,
    device_id: &str,
    view: &AttentionView,
) -> String {
    sha2::Sha256::digest(
        format!(
            "attention.macos\0{}\0{}\0{}\0{}",
            person_id, device_id, view.source_handle, view.view_id,
        )
        .as_bytes(),
    )
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect()
}

pub fn attention_query_fingerprint(
    person_id: PersonId,
    device_id: &str,
    view: &AttentionView,
    observation: Uuid,
    process: Uuid,
) -> Vec<u8> {
    let subject = attention_subject_fingerprint(person_id, device_id, view);
    digest(format!(
        "attention.query\0{}\0{}\0{}\0{}\0{}\0{}",
        subject,
        observation,
        process,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        view.state as u8,
    ))
}

pub fn people_query_fingerprint(
    view: &PeopleView,
    selected_handles: &[String],
    native_subject_fingerprint: &str,
    observation: Uuid,
    process: Uuid,
) -> Vec<u8> {
    digest(format!(
        "people.query\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        view.source_handle,
        selected_handles.join("\0"),
        native_subject_fingerprint,
        observation,
        process,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
    ))
}

pub fn wellbeing_query_fingerprint(
    view: &WellbeingView,
    native_subject_fingerprint: &str,
    observation: Uuid,
    process: Uuid,
) -> Vec<u8> {
    digest(format!(
        "wellbeing.query\0{}\0{}\0{}\0{}\0{}\0{}",
        view.source_handle,
        native_subject_fingerprint,
        observation,
        process,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
    ))
}

/// What a feasibility read asked for. The grant record that stores it belongs
/// to the vault; what shaped the answer belongs here.
pub struct FeasibilityQueryLineage<'a> {
    pub event_handle: &'a str,
    pub evidence_handles: &'a [String],
    pub destination_latitude: f64,
    pub destination_longitude: f64,
    pub event_start_unix_ms: i64,
    pub event_end_unix_ms: i64,
    pub travel_mode: &'a str,
}

pub fn feasibility_query_fingerprint(
    view: &FeasibilityView,
    query: &FeasibilityQueryLineage<'_>,
    native_subject_fingerprint: &str,
    observation: Uuid,
    process: Uuid,
) -> Vec<u8> {
    digest(format!(
        "feasibility.query\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        view.source_handle,
        query.event_handle,
        query.evidence_handles.join("\0"),
        query.destination_latitude,
        query.destination_longitude,
        query.event_start_unix_ms,
        query.event_end_unix_ms,
        query.travel_mode,
        native_subject_fingerprint,
        observation,
        process,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
    ))
}

#[cfg(test)]
mod tests {
    use floe_context_contract::AttentionState;

    use super::*;

    fn attention() -> AttentionView {
        AttentionView {
            schema_version: floe_agent_contract::AGENT_VERSION,
            view_id: "attention.coarse".into(),
            source_handle: "attention.macos:session_idle".into(),
            observed_at_unix_ms: 1_000,
            expires_at_unix_ms: 2_000,
            state: AttentionState::Available,
            confidence_millis: 900,
            evidence_handles: vec!["attention:aggregate".into()],
        }
    }

    #[test]
    fn the_subject_is_stable_and_every_observation_is_its_own_query() {
        let person = PersonId::new();
        let view = attention();
        assert_eq!(
            attention_subject_fingerprint(person, "device", &view),
            attention_subject_fingerprint(person, "device", &view)
        );
        assert_ne!(
            attention_subject_fingerprint(person, "device", &view),
            attention_subject_fingerprint(person, "other", &view)
        );
        assert_ne!(
            attention_query_fingerprint(person, "device", &view, Uuid::new_v4(), Uuid::new_v4()),
            attention_query_fingerprint(person, "device", &view, Uuid::new_v4(), Uuid::new_v4())
        );
    }
}
