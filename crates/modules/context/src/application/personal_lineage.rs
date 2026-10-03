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

use floe_context_contract::{AttentionView, PeopleView, PersonId, WellbeingView};

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
    transform: &floe_context_contract::HealthTransformEvidence,
    native_subject_fingerprint: &str,
    observation: Uuid,
    process: Uuid,
) -> Vec<u8> {
    digest(format!(
        "wellbeing.query\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        view.source_handle,
        native_subject_fingerprint,
        observation,
        process,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        transform.operation_id,
        transform.host_epoch,
        transform
            .output_sha256
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    ))
}
