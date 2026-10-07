use crate::TrustedViewCapability;
use floe_context_contract::{GrantDataCategory, GrantPurpose};
use floe_kernel::AgentFailure;

/// Source View semantics. Assembly copies shipped declarations but does not
/// infer category or purpose authority from names in a product request.
pub fn trusted_view_capability(view_id: &str) -> Result<TrustedViewCapability, AgentFailure> {
    let categories = match view_id {
        "calendar.timeline" => vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        "mail.communication" => vec![GrantDataCategory::Content],
        "people.identity" | "attention.coarse" | "wellbeing.derived" | "work.context"
        | "life.logistics" => vec![GrantDataCategory::Derived],
        _ => return Err(AgentFailure::CapabilityUnavailable),
    };
    Ok(TrustedViewCapability {
        view_id: view_id.to_owned(),
        data_class: floe_context_contract::source_view_data_class(view_id)
            .ok_or(AgentFailure::CapabilityUnavailable)?,
        categories,
        purposes: vec![GrantPurpose::Assistant],
    })
}
pub fn source_view_ids(connector: &str) -> &'static [&'static str] {
    match connector {
        "contacts.apple" | "contacts.android" => &["people.identity"],
        "attention.macos" => &["attention.coarse"],
        "health.apple" => &["wellbeing.derived"],
        "calendar.event_kit" | "calendar.android" | "calendar.google" | "calendar.microsoft" => {
            &["calendar.timeline"]
        }
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        "calendar.fixture" => &["calendar.timeline"],
        "gmail" => &["mail.communication", "life.logistics"],
        "microsoft.mail" => &["mail.communication"],
        "slack.conversations" | "microsoft.teams" | "github.issues" | "google_drive.files" => {
            &["work.context"]
        }
        "home_assistant.states" => &["life.logistics"],
        _ => &[],
    }
}

pub fn remote_connector_ids_for_view(view: &str) -> &'static [&'static str] {
    match view {
        "mail.communication" => &["gmail", "microsoft.mail"],
        "work.context" => &[
            "slack.conversations",
            "microsoft.teams",
            "github.issues",
            "google_drive.files",
        ],
        "life.logistics" => &["gmail", "home_assistant.states"],
        _ => &[],
    }
}
