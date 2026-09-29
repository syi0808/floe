use floe_access::GrantConsumer;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{GrantDataCategory, GrantOperation, GrantPurpose, ProcessingRestriction};
use floe_kernel::PersonId;
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
    let mut consumers = Vec::new();
    if floe_context::manager_direct_remote_view(view_id) {
        consumers.push(
            GrantConsumer::builtin(floe_context::ASSISTANT_CONSUMER)
                .map_err(|_| AgentFailure::InvalidInput)?,
        );
        consumers.sort();
        consumers.dedup();
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
    let native_view = match connector_id {
        floe_access::ATTENTION_CONNECTOR => floe_access::ATTENTION_CONNECTOR,
        floe_access::WELLBEING_CONNECTOR => floe_access::WELLBEING_CONNECTOR,
        "calendar.event_kit" => "calendar.timeline",
        _ => return Err(AgentFailure::InvalidInput),
    };
    if view_id != native_view {
        return Err(AgentFailure::InvalidInput);
    }
    if view_id == "calendar.timeline" {
        return policy_digest(&calendar_policy()?);
    }
    let mut consumers = native_consumers(connector_id)?
        .into_iter()
        .map(GrantConsumer::builtin)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    consumers.sort();
    consumers.dedup();
    policy_digest(&FirstPartyObservePolicy {
        view_id: native_view,
        consumers,
        categories: vec![GrantDataCategory::Derived],
        operation: GrantOperation::Read,
        purpose: GrantPurpose::Assistant,
        processing: ProcessingRestriction::LocalOnly,
    })
}

pub(crate) fn calendar_policy() -> Result<FirstPartyObservePolicy, AgentFailure> {
    let mut policy = policy(
        "calendar.timeline",
        vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
    )?;
    policy.consumers = trusted_shipped_consumers("calendar.timeline")?;
    Ok(policy)
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
        .map(|(view_id, category)| {
            policy(view_id, vec![*category])
        })
        .collect()
}

pub(crate) async fn remote_policies_for_target<Keys: floe_vault::VaultKeyProvider>(
    vault: &floe_vault::EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
) -> Result<Vec<FirstPartyObservePolicy>, AgentFailure> {
    if vault.person_id() != person_id || connection_id.is_empty() {
        return Err(AgentFailure::CapabilityDenied);
    }
    let mut policies = remote_policies(connector_id)?;
    for policy in &mut policies {
        policy
            .consumers
            .extend(trusted_shipped_consumers(policy.view_id)?);
        policy.consumers.sort();
        policy.consumers.dedup();
    }
    Ok(policies)
}

pub(crate) async fn remote_member_policy_digest_for_target<
    Keys: floe_vault::VaultKeyProvider,
>(
    vault: &floe_vault::EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
    view_id: &str,
) -> Result<String, AgentFailure> {
    let policy = remote_policies_for_target(
        vault,
        person_id,
        connector_id,
        connection_id,
    )
    .await?
    .into_iter()
    .find(|policy| policy.view_id == view_id)
    .ok_or(AgentFailure::InvalidInput)?;
    policy_digest(&policy)
}

pub(crate) fn native_consumers(connector_id: &str) -> Result<Vec<String>, AgentFailure> {
    match connector_id {
        "attention.macos" | "contacts.apple" | "contacts.android" | "health.apple"
        | "feasibility.apple" => {}
        _ => return Err(AgentFailure::InvalidInput),
    };
    let mut consumers = Vec::new();
    if floe_context::manager_direct_native_connector(connector_id) {
        consumers.push("assistant".to_owned());
    }
    consumers.sort();
    consumers.dedup();
    Ok(consumers)
}

pub(crate) async fn native_consumers_for_target<Keys: floe_vault::VaultKeyProvider>(
    vault: &floe_vault::EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    _device_id: &str,
) -> Result<Vec<String>, AgentFailure> {
    if vault.person_id() != person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let mut consumers = Vec::new();
    if floe_context::manager_direct_native_connector(connector_id) {
        consumers.push(floe_context::ASSISTANT_CONSUMER.to_owned());
    }
    let capability = match connector_id {
        floe_access::ATTENTION_CONNECTOR => Some("attention.coarse"),
        "contacts.apple" => Some("people.identity"),
        floe_access::WELLBEING_CONNECTOR => Some("wellbeing.derived"),
        "contacts.android" | "feasibility.apple" => None,
        _ => return Err(AgentFailure::InvalidInput),
    };
    if let Some(capability) = capability {
        consumers.extend(
            trusted_shipped_consumers(capability)?
                .into_iter()
                .map(|consumer| consumer.identifier().to_owned()),
        );
    }
    consumers.sort();
    consumers.dedup();
    Ok(consumers)
}

pub(crate) async fn native_member_policy_digest_for_target<
    Keys: floe_vault::VaultKeyProvider,
>(
    vault: &floe_vault::EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    device_id: &str,
) -> Result<String, AgentFailure> {
    let consumers = native_consumers_for_target(vault, person_id, connector_id, device_id)
        .await?
        .into_iter()
        .map(GrantConsumer::builtin)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let view_id = match connector_id {
        floe_access::ATTENTION_CONNECTOR => floe_access::ATTENTION_CONNECTOR,
        floe_access::WELLBEING_CONNECTOR => floe_access::WELLBEING_CONNECTOR,
        _ => return Err(AgentFailure::InvalidInput),
    };
    policy_digest(&FirstPartyObservePolicy {
        view_id,
        consumers,
        categories: vec![GrantDataCategory::Derived],
        operation: GrantOperation::Read,
        purpose: GrantPurpose::Assistant,
        processing: ProcessingRestriction::LocalOnly,
    })
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
    fn calendar_template_grants_shipped_capable_experts() {
        assert_eq!(ids(&calendar_policy().unwrap()).len(), 4);
    }

    #[test]
    fn native_base_consumers_are_manager_only() {
        assert_eq!(native_consumers("attention.macos").unwrap(), ["assistant"]);
        assert_eq!(native_consumers("contacts.apple").unwrap(), ["assistant"]);
        assert_eq!(native_consumers("health.apple").unwrap(), ["assistant"]);
        assert!(
            !native_consumers("attention.macos")
                .unwrap()
                .contains(&"example.test.expert".into())
        );
    }

    #[test]
    fn remote_policy_is_bounded_to_supported_connector_views() {
        let gmail = remote_policies("gmail").unwrap();
        assert_eq!(
            gmail.iter().map(|value| value.view_id).collect::<Vec<_>>(),
            ["mail.communication", "life.logistics"]
        );
        assert_eq!(ids(&gmail[0]), ["assistant"]);
        assert_eq!(ids(&gmail[1]), ["assistant"]);
        assert_eq!(
            ids(&remote_policies("github.issues").unwrap()[0]),
            ["assistant"]
        );
        assert!(remote_policies("unknown.connector").unwrap().is_empty());
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
                    consumer.identifier() == "assistant"
                        || trusted_shipped_consumers(policy.view_id)
                            .unwrap()
                            .contains(consumer)
                }));
                assert_eq!(
                    policy
                        .consumers
                        .iter()
                        .any(|consumer| consumer.identifier() == "assistant"),
                    floe_context::manager_direct_remote_view(policy.view_id)
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
            .push(GrantConsumer::builtin("floe.builtin.commitments").unwrap());
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
