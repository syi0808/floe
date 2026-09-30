use floe_access::GrantConsumer;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{
    GrantDataCategory, GrantOperation, GrantPurpose, ProcessingRestriction,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FirstPartyObservePolicy {
    pub view_id: &'static str,
    pub consumers: Vec<GrantConsumer>,
    pub categories: Vec<GrantDataCategory>,
    pub operation: GrantOperation,
    pub purpose: GrantPurpose,
    pub processing: ProcessingRestriction,
}

pub(crate) fn trusted_shipped_consumers(
    capability: &str,
) -> Result<Vec<GrantConsumer>, AgentFailure> {
    let mut consumers = Vec::new();
    for manifest in floe_experts_builtin::manifests() {
        manifest.validate()?;
        if manifest
            .source_requirements
            .iter()
            .any(|requirement| requirement.capability == capability)
        {
            consumers.push(
                GrantConsumer::builtin(&manifest.package.id)
                    .map_err(|_| AgentFailure::InvalidInput)?,
            );
        }
    }
    consumers.sort();
    consumers.dedup();
    Ok(consumers)
}

fn policy(
    view_id: &'static str,
    categories: Vec<GrantDataCategory>,
) -> Result<FirstPartyObservePolicy, AgentFailure> {
    let consumers = trusted_shipped_consumers(view_id)?;
    if consumers.is_empty() {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    Ok(FirstPartyObservePolicy {
        view_id,
        consumers,
        categories,
        operation: GrantOperation::Read,
        purpose: GrantPurpose::Assistant,
        processing: ProcessingRestriction::LocalOnly,
    })
}

pub(crate) fn policy_digest(policy: &FirstPartyObservePolicy) -> Result<String, AgentFailure> {
    let mut consumers: Vec<&str> = policy
        .consumers
        .iter()
        .map(GrantConsumer::identifier)
        .collect();
    consumers.sort_unstable();
    let mut categories = policy.categories.clone();
    categories.sort();
    if policy.view_id.is_empty()
        || policy.view_id.len() > 128
        || consumers.windows(2).any(|pair| pair[0] == pair[1])
        || categories.is_empty()
        || categories.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(AgentFailure::InvalidInput);
    }
    let representation = serde_json::to_vec(&(
        "floe.first-party-observe-policy.sha256.v2",
        policy.view_id,
        consumers,
        categories,
        policy.operation,
        policy.purpose,
        &policy.processing,
    ))
    .map_err(|_| AgentFailure::InvalidInput)?;
    if representation.len() > 4096 {
        return Err(AgentFailure::InvalidInput);
    }
    let bytes: [u8; 32] = Sha256::digest(representation).into();
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn member_policy_digest(
    connector_id: &str,
    view_id: &str,
) -> Result<String, AgentFailure> {
    if let Some(policy) = remote_policies(connector_id)?
        .into_iter()
        .find(|policy| policy.view_id == view_id)
    {
        return policy_digest(&policy);
    }
    if connector_id == "calendar.event_kit" && view_id == "calendar.timeline" {
        return policy_digest(&calendar_policy()?);
    }
    let policy = personal_policy(connector_id)?;
    if view_id != policy.view_id {
        return Err(AgentFailure::InvalidInput);
    }
    policy_digest(&policy)
}

pub(crate) fn personal_policy(connector_id: &str) -> Result<FirstPartyObservePolicy, AgentFailure> {
    let view_id =
        crate::personal_source_spec::PersonalSourceSpec::for_connector(connector_id)?.view;
    if connector_id == "contacts.android" {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    policy(view_id, vec![GrantDataCategory::Derived])
}

pub(crate) fn calendar_policy() -> Result<FirstPartyObservePolicy, AgentFailure> {
    policy(
        "calendar.timeline",
        vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
    )
}

pub(crate) fn remote_policies(
    connector_id: &str,
) -> Result<Vec<FirstPartyObservePolicy>, AgentFailure> {
    let views: &[(&str, GrantDataCategory)] = match connector_id {
        "gmail" => &[
            ("mail.communication", GrantDataCategory::Content),
            ("life.logistics", GrantDataCategory::Derived),
        ],
        "microsoft.mail" => &[("mail.communication", GrantDataCategory::Content)],
        "slack.conversations" | "microsoft.teams" | "github.issues" | "google_drive.files" => {
            &[("work.context", GrantDataCategory::Derived)]
        }
        "home_assistant.states" => &[("life.logistics", GrantDataCategory::Derived)],
        "calendar.google" | "calendar.microsoft" => return Ok(vec![calendar_policy()?]),
        _ => return Ok(Vec::new()),
    };
    views
        .iter()
        .map(|(view_id, category)| policy(view_id, vec![*category]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_kernel::PersonId;

    fn ids(policy: &FirstPartyObservePolicy) -> Vec<&str> {
        policy
            .consumers
            .iter()
            .map(GrantConsumer::identifier)
            .collect()
    }

    #[test]
    fn calendar_template_grants_shipped_capable_experts() {
        assert_eq!(ids(&calendar_policy().unwrap()).len(), 4);
    }

    #[test]
    fn supported_views_have_exact_shipped_readers_and_android_is_unavailable() {
        for (view, expected) in [
            ("people.identity", vec!["floe.builtin.relationships"]),
            ("attention.coarse", vec!["floe.builtin.focus-attention"]),
            ("wellbeing.derived", vec!["floe.builtin.wellbeing"]),
            (
                "mail.communication",
                vec!["floe.builtin.commitments", "floe.builtin.communication"],
            ),
            (
                "work.context",
                vec!["floe.builtin.focus-attention", "floe.builtin.work-context"],
            ),
            ("life.logistics", vec!["floe.builtin.life-logistics"]),
            (
                "calendar.timeline",
                vec![
                    "floe.builtin.commitments",
                    "floe.builtin.focus-attention",
                    "floe.builtin.schedule",
                    "floe.builtin.wellbeing",
                ],
            ),
        ] {
            let policy = policy(view, vec![GrantDataCategory::Derived]).unwrap();
            assert_eq!(ids(&policy), expected);
        }
        assert_eq!(
            personal_policy("contacts.android"),
            Err(AgentFailure::CapabilityUnavailable)
        );
        assert!(policy("unsupported.view", vec![GrantDataCategory::Derived]).is_err());
    }

    #[test]
    fn personal_policy_uses_canonical_view_and_trusted_consumers() {
        for connector in ["contacts.apple", "attention.macos", "health.apple"] {
            let policy = personal_policy(connector).unwrap();
            assert!(policy.consumers.iter().all(|consumer| {
                trusted_shipped_consumers(policy.view_id)
                    .unwrap()
                    .contains(consumer)
            }));
            assert_eq!(
                member_policy_digest(connector, policy.view_id).unwrap(),
                policy_digest(&policy).unwrap()
            );
            assert!(member_policy_digest(connector, connector).is_err());
        }
        assert_eq!(
            personal_policy("contacts.apple").unwrap().view_id,
            floe_context_contract::PEOPLE_VIEW_ID
        );
        assert_eq!(
            personal_policy("attention.macos").unwrap().view_id,
            floe_context_contract::ATTENTION_VIEW_ID
        );
        assert_eq!(
            personal_policy("health.apple").unwrap().view_id,
            floe_context_contract::WELLBEING_VIEW_ID
        );
    }

    #[test]
    fn personal_consumers_are_only_trusted_shipped_experts() {
        for connector in ["attention.macos", "contacts.apple", "health.apple"] {
            let policy = personal_policy(connector).unwrap();
            assert!(!ids(&policy).contains(&"assistant"));
            assert_eq!(
                policy.consumers,
                trusted_shipped_consumers(policy.view_id).unwrap()
            );
            assert!(!ids(&policy).contains(&"example.test.expert"));
        }
    }

    #[test]
    fn remote_policy_is_bounded_to_supported_connector_views() {
        let gmail = remote_policies("gmail").unwrap();
        assert_eq!(
            gmail.iter().map(|value| value.view_id).collect::<Vec<_>>(),
            ["mail.communication", "life.logistics"]
        );
        for remote in [
            &gmail[0],
            &gmail[1],
            &remote_policies("github.issues").unwrap()[0],
        ] {
            let mut expected = trusted_shipped_consumers(remote.view_id).unwrap();
            expected.sort();
            expected.dedup();
            assert_eq!(remote.consumers, expected);
        }
        assert!(remote_policies("unknown.connector").unwrap().is_empty());
    }

    #[test]
    fn every_member_digest_matches_the_activation_policy() {
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
                assert_eq!(
                    member_policy_digest(connector, policy.view_id).unwrap(),
                    policy_digest(&policy).unwrap()
                );
            }
        }
        let calendar = calendar_policy().unwrap();
        assert_eq!(
            member_policy_digest("calendar.event_kit", calendar.view_id).unwrap(),
            policy_digest(&calendar).unwrap()
        );
        for connector in ["contacts.apple", "attention.macos", "health.apple"] {
            let policy = personal_policy(connector).unwrap();
            assert_eq!(
                member_policy_digest(connector, policy.view_id).unwrap(),
                policy_digest(&policy).unwrap()
            );
        }
    }

    #[test]
    fn policy_never_default_grants_extensions_or_wildcards() {
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
                    trusted_shipped_consumers(policy.view_id)
                        .unwrap()
                        .contains(consumer)
                }));
                assert_eq!(
                    policy
                        .consumers
                        .iter()
                        .any(|consumer| consumer.identifier() == "assistant"),
                    false
                );
            }
        }
    }

    #[test]
    fn digest_binds_exact_prospective_permission_scope() {
        let policy = remote_policies("microsoft.mail").unwrap().remove(0);
        let digest = policy_digest(&policy).unwrap();
        assert_eq!(digest.len(), 64);
        let mut changed = policy.clone();
        changed
            .consumers
            .push(GrantConsumer::builtin("example.calendar.extension").unwrap());
        assert_ne!(policy_digest(&changed).unwrap(), digest);
        let mut changed = policy.clone();
        changed.categories = vec![GrantDataCategory::Derived];
        assert_ne!(policy_digest(&changed).unwrap(), digest);
        let mut changed = policy.clone();
        changed.operation = GrantOperation::Suggestion;
        assert_ne!(policy_digest(&changed).unwrap(), digest);
        let mut changed = policy.clone();
        changed.purpose = GrantPurpose::Scheduling;
        assert_ne!(policy_digest(&changed).unwrap(), digest);
        let mut changed = policy.clone();
        changed.processing = ProcessingRestriction::ApprovedRecipient {
            recipient: "model.example".into(),
            categories: vec![GrantDataCategory::Content],
        };
        assert_ne!(policy_digest(&changed).unwrap(), digest);
    }

    #[test]
    fn digest_is_canonical_under_consumer_and_category_order() {
        let mut policy = calendar_policy().unwrap();
        let digest = policy_digest(&policy).unwrap();
        policy.consumers.reverse();
        policy.categories.reverse();
        assert_eq!(policy_digest(&policy).unwrap(), digest);
        assert_eq!(
            policy_digest(&remote_policies("calendar.google").unwrap().remove(0)).unwrap(),
            digest
        );
    }

    #[test]
    fn calendar_consumers_match_trusted_shipped_capability_declarations() {
        let expected = floe_experts_builtin::manifests()
            .into_iter()
            .filter(|manifest| {
                manifest
                    .source_requirements
                    .iter()
                    .any(|requirement| requirement.capability == "calendar.timeline")
            })
            .map(|manifest| manifest.package.id)
            .collect::<std::collections::BTreeSet<_>>();
        let actual = calendar_policy()
            .unwrap()
            .consumers
            .into_iter()
            .map(|consumer| consumer.identifier().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(actual, expected);
        assert_eq!(
            actual,
            [
                "floe.builtin.commitments".to_owned(),
                "floe.builtin.focus-attention".to_owned(),
                "floe.builtin.schedule".to_owned(),
                "floe.builtin.wellbeing".to_owned(),
            ]
            .into_iter()
            .collect()
        );
        assert!(!actual.contains("assistant"));
        assert!(!actual.contains("example.test.expert"));
    }

    #[test]
    fn shipped_permission_digest_does_not_depend_on_registry_state() {
        let before = calendar_policy().unwrap();
        let before_fingerprint = policy_digest(&before).unwrap();
        let mut registry = floe_experts::AgentRegistry::new(uuid::Uuid::new_v4());
        let person = PersonId::new();
        let manifest = floe_experts_builtin::manifests()
            .into_iter()
            .find(|manifest| manifest.package.id == "floe.builtin.schedule")
            .unwrap();
        registry
            .install_bundle(
                person,
                &floe_experts::ExpertInstallOperation {
                    instance_id: registry.snapshot().instance_id,
                    expected_revision: 0,
                    operation_id: uuid::Uuid::new_v4(),
                },
                &[manifest],
            )
            .unwrap();
        let mut extension = floe_experts_builtin::manifests()
            .into_iter()
            .find(|manifest| manifest.package.id == "floe.builtin.schedule")
            .unwrap();
        extension.package.id = "example.calendar.extension".into();
        extension.definition.card.id = extension.package.id.clone();
        extension.publisher = "example".into();
        extension.validate().unwrap();
        registry
            .install_bundle(
                person,
                &floe_experts::ExpertInstallOperation {
                    instance_id: registry.snapshot().instance_id,
                    expected_revision: registry.snapshot().revision,
                    operation_id: uuid::Uuid::new_v4(),
                },
                &[extension.clone()],
            )
            .unwrap();
        let snapshot = registry.snapshot();
        let installation = snapshot
            .installations
            .iter()
            .find(|installation| installation.package == extension.package)
            .unwrap();
        let assignment = snapshot
            .assignments
            .iter()
            .find(|assignment| assignment.installation_id == installation.id)
            .unwrap();
        registry
            .replace_binding(
                person,
                uuid::Uuid::new_v4(),
                floe_experts::ExpertBindingCommand {
                    assignment_id: assignment.id,
                    package: extension.package.clone(),
                    definition_revision: extension.definition.definition_revision,
                    requirement_key: "floe.source.calendar".into(),
                    expected_binding_revision: assignment.binding.revision,
                    selected: vec![floe_context_contract::SourceSelectionReference {
                        connector_id: floe_context_contract::ConnectorId::try_new(
                            "calendar.event_kit",
                        )
                        .unwrap(),
                        connection_id: floe_context_contract::ConnectionId::try_new("extension")
                            .unwrap(),
                        execution_owner_id: floe_context_contract::ExecutionOwnerId::try_new(
                            "device",
                        )
                        .unwrap(),
                        capability_id: "calendar.timeline".into(),
                        resource: floe_access::native_calendar_resource("extension").unwrap(),
                        contract_version: 1,
                    }],
                },
            )
            .unwrap();
        assert_eq!(calendar_policy().unwrap(), before);
        assert_eq!(
            policy_digest(&calendar_policy().unwrap()).unwrap(),
            before_fingerprint
        );
        assert_eq!(
            trusted_shipped_consumers("calendar.timeline").unwrap(),
            before.consumers
        );
    }
}
