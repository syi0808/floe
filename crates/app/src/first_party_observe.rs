use floe_access::GrantConsumer;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{GrantDataCategory, GrantPurpose};
use floe_experts_builtin::{BuiltinContextSource, BuiltinExpertKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceProcessingPolicy {
    LocalOnly,
    PairedSourceRecipient,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FirstPartyObservePolicy {
    pub view_id: &'static str,
    pub consumers: Vec<GrantConsumer>,
    pub categories: Vec<GrantDataCategory>,
    pub purpose: GrantPurpose,
    pub source_processing: SourceProcessingPolicy,
}

fn builtin_consumers(source: BuiltinContextSource) -> Result<Vec<GrantConsumer>, AgentFailure> {
    let mut consumers = BuiltinExpertKind::ALL
        .into_iter()
        .filter(|kind| kind.required_sources().contains(&source))
        .map(|kind| GrantConsumer::builtin(kind.package_id()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    consumers.sort();
    consumers.dedup();
    if consumers.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(consumers)
}

fn policy(
    view_id: &'static str,
    source: BuiltinContextSource,
    categories: Vec<GrantDataCategory>,
    source_processing: SourceProcessingPolicy,
) -> Result<FirstPartyObservePolicy, AgentFailure> {
    Ok(FirstPartyObservePolicy {
        view_id,
        consumers: builtin_consumers(source)?,
        categories,
        purpose: GrantPurpose::Assistant,
        source_processing,
    })
}

pub(crate) fn calendar_policy() -> Result<FirstPartyObservePolicy, AgentFailure> {
    policy(
        "calendar.timeline",
        BuiltinContextSource::Calendar,
        vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        SourceProcessingPolicy::LocalOnly,
    )
}

pub(crate) fn remote_policies(
    connector_id: &str,
) -> Result<Vec<FirstPartyObservePolicy>, AgentFailure> {
    let views: &[(&str, BuiltinContextSource, GrantDataCategory)] = match connector_id {
        "gmail" => &[
            (
                "mail.communication",
                BuiltinContextSource::Mail,
                GrantDataCategory::Content,
            ),
            (
                "life.logistics",
                BuiltinContextSource::Logistics,
                GrantDataCategory::Derived,
            ),
        ],
        "microsoft.mail" => &[((
            "mail.communication",
            BuiltinContextSource::Mail,
            GrantDataCategory::Content,
        ))],
        "slack.conversations" | "microsoft.teams" | "github.issues" | "google_drive.files" => &[((
            "work.context",
            BuiltinContextSource::WorkContext,
            GrantDataCategory::Derived,
        ))],
        "home_assistant.states" => &[((
            "life.logistics",
            BuiltinContextSource::Logistics,
            GrantDataCategory::Derived,
        ))],
        "calendar.google" | "calendar.microsoft" => {
            return Ok(vec![FirstPartyObservePolicy {
                source_processing: SourceProcessingPolicy::PairedSourceRecipient,
                ..calendar_policy()?
            }]);
        }
        _ => return Ok(Vec::new()),
    };
    views
        .iter()
        .map(|(view_id, source, category)| {
            policy(
                view_id,
                *source,
                vec![*category],
                SourceProcessingPolicy::PairedSourceRecipient,
            )
        })
        .collect()
}

pub(crate) fn native_consumers(connector_id: &str) -> Result<Vec<String>, AgentFailure> {
    let consumers: &[&str] = match connector_id {
        "attention.macos" => &["assistant", "attention.expert"],
        "contacts.apple" | "contacts.android" => &["assistant", "contacts.expert"],
        "health.apple" | "feasibility.apple" => &["assistant"],
        _ => return Err(AgentFailure::InvalidInput),
    };
    Ok(consumers.iter().map(|value| (*value).to_owned()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(policy: &FirstPartyObservePolicy) -> Vec<&str> {
        policy
            .consumers
            .iter()
            .map(GrantConsumer::identifier)
            .collect()
    }

    #[test]
    fn calendar_consumers_come_from_current_builtin_readers() {
        assert_eq!(
            ids(&calendar_policy().unwrap()),
            [
                "floe.builtin.commitments",
                "floe.builtin.focus-attention",
                "floe.builtin.schedule",
                "floe.builtin.wellbeing",
            ]
        );
    }

    #[test]
    fn remote_policy_is_bounded_to_supported_connector_views() {
        let gmail = remote_policies("gmail").unwrap();
        assert_eq!(
            gmail.iter().map(|value| value.view_id).collect::<Vec<_>>(),
            ["mail.communication", "life.logistics"]
        );
        assert_eq!(
            ids(&gmail[0]),
            ["floe.builtin.commitments", "floe.builtin.communication"]
        );
        assert_eq!(ids(&gmail[1]), ["floe.builtin.life-logistics"]);
        assert_eq!(
            ids(&remote_policies("github.issues").unwrap()[0]),
            ["floe.builtin.focus-attention", "floe.builtin.work-context"]
        );
        assert!(remote_policies("unknown.connector").unwrap().is_empty());
    }

    #[test]
    fn policy_never_default_grants_extensions_or_assistant_wildcards() {
        for connector in [
            "gmail",
            "microsoft.mail",
            "slack.conversations",
            "microsoft.teams",
            "github.issues",
            "google_drive.files",
            "home_assistant.states",
            "calendar.google",
            "calendar.microsoft",
        ] {
            for policy in remote_policies(connector).unwrap() {
                assert!(policy.consumers.iter().all(|consumer| {
                    matches!(consumer, GrantConsumer::Builtin(id) if id.starts_with("floe.builtin."))
                }));
            }
        }
    }
}
