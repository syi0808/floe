use floe_access::GrantConsumer;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{GrantDataCategory, GrantPurpose};
use floe_kernel::PersonId;
use sha2::{Digest, Sha256};

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

pub(crate) fn selected_shipped_consumers(
    snapshot: Option<&floe_experts::RegistrySnapshot>,
    person_id: PersonId,
    capability: &str,
    connector_id: &str,
    connection_id: &str,
    execution_owner_id: &str,
    resource: &str,
) -> Result<Vec<GrantConsumer>, AgentFailure> {
    let Some(snapshot) = snapshot else {
        return Ok(Vec::new());
    };
    let registry = floe_experts::AgentRegistry::restore(snapshot.clone(), snapshot.instance_id)?;
    let shipped = floe_experts_builtin::manifests();
    let mut consumers = Vec::new();
    for assignment in &snapshot.assignments {
        if assignment.person_id != person_id || !assignment.enabled {
            continue;
        }
        let installation = snapshot
            .installations
            .iter()
            .find(|entry| entry.id == assignment.installation_id)
            .ok_or(AgentFailure::StorageUnavailable)?;
        if !installation.enabled {
            continue;
        }
        let manifest = snapshot
            .manifests
            .iter()
            .find(|entry| entry.package == installation.package)
            .ok_or(AgentFailure::StorageUnavailable)?;
        if !shipped.contains(manifest) {
            continue;
        }
        registry.validate_active_assignment(person_id, assignment.id)?;
        if assignment.binding.entries.iter().any(|entry| {
            entry.capability == capability
                && entry.selected.iter().any(|selected| {
                    selected.connector_id.as_str() == connector_id
                        && selected.connection_id.as_str() == connection_id
                        && selected.execution_owner_id.as_str() == execution_owner_id
                        && selected.resource.as_str() == resource
                })
        }) {
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
    source_processing: SourceProcessingPolicy,
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
        purpose: GrantPurpose::Assistant,
        source_processing,
    })
}

pub(crate) fn policy_fingerprint(policy: &FirstPartyObservePolicy) -> Result<String, AgentFailure> {
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
        "floe.first-party-observe-policy.sha256.v1",
        policy.view_id,
        consumers,
        categories,
        policy.purpose,
        match policy.source_processing {
            SourceProcessingPolicy::LocalOnly => "local-only",
            SourceProcessingPolicy::PairedSourceRecipient => "paired-source-recipient",
        },
    ))
    .map_err(|_| AgentFailure::InvalidInput)?;
    if representation.len() > 4096 {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(format!("{:x}", Sha256::digest(representation)))
}

pub(crate) fn member_policy_fingerprint(
    connector_id: &str,
    view_id: &str,
) -> Result<String, AgentFailure> {
    if let Some(policy) = remote_policies(connector_id)?
        .into_iter()
        .find(|policy| policy.view_id == view_id)
    {
        return policy_fingerprint(&policy);
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
        return policy_fingerprint(&calendar_policy()?);
    }
    let mut consumers = native_consumers(connector_id)?
        .into_iter()
        .map(GrantConsumer::builtin)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    consumers.sort();
    consumers.dedup();
    policy_fingerprint(&FirstPartyObservePolicy {
        view_id: native_view,
        consumers,
        categories: vec![GrantDataCategory::Derived],
        purpose: GrantPurpose::Assistant,
        source_processing: SourceProcessingPolicy::LocalOnly,
    })
}

pub(crate) fn calendar_policy() -> Result<FirstPartyObservePolicy, AgentFailure> {
    policy(
        "calendar.timeline",
        vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        SourceProcessingPolicy::LocalOnly,
    )
}

pub(crate) async fn native_calendar_policy_for_target<Keys: floe_vault::VaultKeyProvider>(
    vault: &floe_vault::EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
    device_id: &str,
    resources: &[String],
) -> Result<FirstPartyObservePolicy, AgentFailure> {
    if vault.person_id() != person_id
        || connector_id != "calendar.event_kit"
        || connection_id.is_empty()
        || resources.is_empty()
    {
        return Err(AgentFailure::InvalidInput);
    }
    let snapshot = vault.expert_registry().await?;
    let mut consumers: Option<Vec<GrantConsumer>> = None;
    for resource in resources {
        let selected = selected_shipped_consumers(
            snapshot.as_ref(),
            person_id,
            "calendar.timeline",
            connector_id,
            connection_id,
            device_id,
            resource,
        )?;
        consumers = Some(match consumers {
            None => selected,
            Some(previous) => previous
                .into_iter()
                .filter(|consumer| selected.contains(consumer))
                .collect(),
        });
    }
    let mut policy = calendar_policy()?;
    policy.consumers = consumers.unwrap_or_default();
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
        .map(|(view_id, category)| {
            policy(
                view_id,
                vec![*category],
                SourceProcessingPolicy::PairedSourceRecipient,
            )
        })
        .collect()
}

pub(crate) async fn remote_policies_for_target<Keys: floe_vault::VaultKeyProvider>(
    vault: &floe_vault::EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
    calendar_resource: Option<&str>,
) -> Result<Vec<FirstPartyObservePolicy>, AgentFailure> {
    if vault.person_id() != person_id || connection_id.is_empty() {
        return Err(AgentFailure::CapabilityDenied);
    }
    let snapshot = vault.expert_registry().await?;
    let execution_owner = match vault.remote_pinned_producer().await {
        Ok(producer) => Some(producer.execution_owner),
        Err(AgentFailure::NotFound) => None,
        Err(failure) => return Err(failure),
    };
    let mut policies = remote_policies(connector_id)?;
    for policy in &mut policies {
        let resource = if policy.view_id == "calendar.timeline" {
            calendar_resource
                .ok_or(AgentFailure::InvalidInput)?
                .to_owned()
        } else {
            floe_context::remote_view_resource(policy.view_id, connection_id)
        };
        if let Some(execution_owner) = &execution_owner {
            policy.consumers.extend(selected_shipped_consumers(
                snapshot.as_ref(),
                person_id,
                policy.view_id,
                connector_id,
                connection_id,
                execution_owner,
                &resource,
            )?);
        }
        policy.consumers.sort();
        policy.consumers.dedup();
    }
    Ok(policies)
}

pub(crate) async fn remote_member_policy_fingerprint_for_target<
    Keys: floe_vault::VaultKeyProvider,
>(
    vault: &floe_vault::EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
    view_id: &str,
    resource: &str,
) -> Result<String, AgentFailure> {
    let calendar_resource = (view_id == "calendar.timeline").then_some(resource);
    let policy = remote_policies_for_target(
        vault,
        person_id,
        connector_id,
        connection_id,
        calendar_resource,
    )
    .await?
    .into_iter()
    .find(|policy| policy.view_id == view_id)
    .ok_or(AgentFailure::InvalidInput)?;
    policy_fingerprint(&policy)
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
    device_id: &str,
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
        let candidate =
            floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
                person_id,
                device_id,
                capability,
                contract_version: 1,
                remote_connections: &[],
                remote_execution_owner: None,
                calendar_connection: None,
            })?
            .into_iter()
            .find(|candidate| candidate.reference.connector_id.as_str() == connector_id)
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        consumers.extend(
            selected_shipped_consumers(
                vault.expert_registry().await?.as_ref(),
                person_id,
                capability,
                connector_id,
                candidate.reference.connection_id.as_str(),
                candidate.reference.execution_owner_id.as_str(),
                candidate.reference.resource.as_str(),
            )?
            .into_iter()
            .map(|consumer| consumer.identifier().to_owned()),
        );
    }
    consumers.sort();
    consumers.dedup();
    Ok(consumers)
}

pub(crate) async fn native_member_policy_fingerprint_for_target<
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
    policy_fingerprint(&FirstPartyObservePolicy {
        view_id,
        consumers,
        categories: vec![GrantDataCategory::Derived],
        purpose: GrantPurpose::Assistant,
        source_processing: SourceProcessingPolicy::LocalOnly,
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
    fn calendar_template_grants_no_unselected_expert() {
        assert!(ids(&calendar_policy().unwrap()).is_empty());
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
                assert!(
                    policy
                        .consumers
                        .iter()
                        .all(|consumer| consumer.identifier() == "assistant")
                );
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
    fn fingerprint_binds_exact_prospective_policy_scope() {
        let policy = remote_policies("microsoft.mail").unwrap().remove(0);
        let fingerprint = policy_fingerprint(&policy).unwrap();
        assert_eq!(fingerprint.len(), 64);
        let mut changed = policy.clone();
        changed
            .consumers
            .push(GrantConsumer::builtin("floe.builtin.commitments").unwrap());
        assert_ne!(policy_fingerprint(&changed).unwrap(), fingerprint);
        let mut changed = policy.clone();
        changed.categories = vec![GrantDataCategory::Derived];
        assert_ne!(policy_fingerprint(&changed).unwrap(), fingerprint);
        let mut changed = policy.clone();
        changed.purpose = GrantPurpose::Scheduling;
        assert_ne!(policy_fingerprint(&changed).unwrap(), fingerprint);
        let mut changed = policy.clone();
        changed.source_processing = SourceProcessingPolicy::LocalOnly;
        assert_ne!(policy_fingerprint(&changed).unwrap(), fingerprint);
    }

    #[test]
    fn shipped_consumer_requires_active_exact_binding() {
        use floe_connections::{
            ConnectionState, ConnectorConnectionSnapshot, ConnectorDescriptor, ConnectorSnapshot,
            ExecutionLocation,
        };

        let person = PersonId::new();
        let instance_id = uuid::Uuid::new_v4();
        let manifest = floe_experts_builtin::manifests()
            .into_iter()
            .find(|manifest| manifest.package.id == "floe.builtin.commitments")
            .unwrap();
        let requirement = manifest
            .source_requirements
            .iter()
            .find(|requirement| requirement.capability == "mail.communication")
            .unwrap()
            .clone();
        let mut registry = floe_experts::AgentRegistry::new(instance_id);
        registry
            .install_bundle(
                person,
                &floe_experts::ExpertInstallOperation {
                    instance_id,
                    expected_revision: 0,
                    operation_id: uuid::Uuid::new_v4(),
                },
                &[manifest.clone()],
            )
            .unwrap();
        let snapshot = registry.snapshot();
        assert!(
            selected_shipped_consumers(
                Some(&snapshot),
                person,
                "mail.communication",
                "gmail",
                "account-a",
                "server:source",
                &floe_context::remote_view_resource("mail.communication", "account-a"),
            )
            .unwrap()
            .is_empty()
        );
        registry
            .replace_binding(
                person,
                uuid::Uuid::new_v4(),
                floe_experts::ExpertBindingCommand {
                    assignment_id: snapshot.assignments[0].id,
                    package: manifest.package.clone(),
                    definition_revision: manifest.definition.definition_revision,
                    requirement_key: requirement.key.clone(),
                    expected_binding_revision: 1,
                    selected: vec![floe_context_contract::SourceSelectionReference {
                        connector_id: floe_context_contract::ConnectorId::try_new("gmail").unwrap(),
                        connection_id: floe_context_contract::ConnectionId::try_new("account-a")
                            .unwrap(),
                        execution_owner_id: floe_context_contract::ExecutionOwnerId::try_new(
                            "server:source",
                        )
                        .unwrap(),
                        capability_id: "mail.communication".into(),
                        resource: floe_context_contract::ResourceHandle::try_new(
                            floe_context::remote_view_resource("mail.communication", "account-a"),
                        )
                        .unwrap(),
                        contract_version: 1,
                    }],
                },
            )
            .unwrap();
        let bound = registry.snapshot();
        let new_connection = ConnectorSnapshot {
            descriptor: ConnectorDescriptor {
                schema_version: 1,
                id: "gmail".into(),
                version: "1".into(),
                provider: "Google Mail".into(),
                execution: ExecutionLocation::Server,
                capabilities: vec![],
                views: vec![],
            },
            connection: ConnectorConnectionSnapshot {
                schema_version: 1,
                connector_id: "gmail".into(),
                connection_id: Some("account-b".into()),
                person_id: Some(person.to_string()),
                device_binding: None,
                state: ConnectionState::Ready,
                granted_scopes: vec![],
                observed_at_unix_ms: 1,
                last_success_at_unix_ms: None,
                last_failure: None,
            },
            views: vec![],
        };
        let new_candidate =
            floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
                person_id: person,
                device_id: "mac-local",
                capability: "mail.communication",
                contract_version: 1,
                remote_connections: &[new_connection],
                remote_execution_owner: Some("server:source"),
                calendar_connection: None,
            })
            .unwrap()
            .remove(0);
        assert_eq!(new_candidate.reference.connection_id.as_str(), "account-b");
        assert_eq!(bound.assignments[0].binding.revision, 2);
        assert_eq!(
            bound.assignments[0]
                .binding
                .entries
                .iter()
                .find(|entry| entry.capability == "mail.communication")
                .unwrap()
                .selected[0]
                .connection_id
                .as_str(),
            "account-a"
        );
        let selected = selected_shipped_consumers(
            Some(&bound),
            person,
            "mail.communication",
            "gmail",
            "account-a",
            "server:source",
            &floe_context::remote_view_resource("mail.communication", "account-a"),
        )
        .unwrap();
        assert_eq!(
            selected
                .iter()
                .map(GrantConsumer::identifier)
                .collect::<Vec<_>>(),
            ["floe.builtin.commitments"]
        );
        assert!(
            selected_shipped_consumers(
                Some(&bound),
                person,
                "mail.communication",
                "gmail",
                "account-a",
                "server:other",
                &floe_context::remote_view_resource("mail.communication", "account-a"),
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            selected_shipped_consumers(
                Some(&bound),
                person,
                "mail.communication",
                "gmail",
                "account-b",
                "server:source",
                &floe_context::remote_view_resource("mail.communication", "account-b"),
            )
            .unwrap()
            .is_empty()
        );
        let mut before_policy = remote_policies("gmail").unwrap().remove(0);
        before_policy.consumers.extend(selected);
        before_policy.consumers.sort();
        let before_fingerprint = policy_fingerprint(&before_policy).unwrap();
        registry
            .replace_binding(
                person,
                uuid::Uuid::new_v4(),
                floe_experts::ExpertBindingCommand {
                    assignment_id: snapshot.assignments[0].id,
                    package: manifest.package,
                    definition_revision: manifest.definition.definition_revision,
                    requirement_key: requirement.key,
                    expected_binding_revision: 2,
                    selected: vec![floe_context_contract::SourceSelectionReference {
                        connector_id: floe_context_contract::ConnectorId::try_new("gmail").unwrap(),
                        connection_id: floe_context_contract::ConnectionId::try_new("account-b")
                            .unwrap(),
                        execution_owner_id: floe_context_contract::ExecutionOwnerId::try_new(
                            "server:source",
                        )
                        .unwrap(),
                        capability_id: "mail.communication".into(),
                        resource: floe_context_contract::ResourceHandle::try_new(
                            floe_context::remote_view_resource("mail.communication", "account-b"),
                        )
                        .unwrap(),
                        contract_version: 1,
                    }],
                },
            )
            .unwrap();
        let rebound = registry.snapshot();
        let mut after_policy = remote_policies("gmail").unwrap().remove(0);
        after_policy.consumers.extend(
            selected_shipped_consumers(
                Some(&rebound),
                person,
                "mail.communication",
                "gmail",
                "account-a",
                "server:source",
                &floe_context::remote_view_resource("mail.communication", "account-a"),
            )
            .unwrap(),
        );
        assert_ne!(
            policy_fingerprint(&after_policy).unwrap(),
            before_fingerprint
        );
    }
}
