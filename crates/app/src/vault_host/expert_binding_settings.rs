use std::collections::HashSet;

use floe_agent_contract::{AgentFailure, PackageKind, PackageRef};
use floe_kernel::PersonId;
use uuid::Uuid;

use super::OpenVault;

struct CurrentCandidates {
    catalog: crate::ExpertCandidateCatalog,
    live: Vec<floe_context::SourceCandidate>,
    package: PackageRef,
    definition_revision: u64,
}

struct BindingContext {
    assignment_id: Uuid,
    requirement_key: String,
    capability: String,
    contract_version: u32,
    selected: Vec<floe_context_contract::SourceSelectionReference>,
    binding_revision: u64,
    package: PackageRef,
    definition_revision: u64,
}

async fn binding_context<Keys: floe_vault::VaultKeyProvider>(
    open: &OpenVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    assignment_id: Uuid,
    requirement_key: &str,
) -> Result<BindingContext, AgentFailure> {
    if person_id != open.vault.person_id()
        || device_id.is_empty()
        || assignment_id.is_nil()
        || requirement_key.is_empty()
        || requirement_key.len() > 128
    {
        return Err(AgentFailure::InvalidInput);
    }
    let snapshot = open
        .vault
        .expert_registry()
        .await?
        .ok_or(AgentFailure::NotFound)?;
    let registry =
        floe_experts::AgentRegistry::restore(snapshot.clone(), open.vault.registry_instance_id())?;
    let assignment = snapshot
        .assignments
        .iter()
        .find(|entry| entry.id == assignment_id && entry.person_id == person_id)
        .ok_or(AgentFailure::NotFound)?;
    let installation = snapshot
        .installations
        .iter()
        .find(|entry| entry.id == assignment.installation_id)
        .ok_or(AgentFailure::NotFound)?;
    let manifest = snapshot
        .manifests
        .iter()
        .find(|entry| entry.package == installation.package)
        .ok_or(AgentFailure::NotFound)?;
    registry.resolve_assignment(
        snapshot.instance_id,
        person_id,
        assignment_id,
        &installation.package,
        manifest.definition.definition_revision,
    )?;
    let requirement = manifest
        .source_requirements
        .iter()
        .find(|entry| entry.key == requirement_key)
        .ok_or(AgentFailure::NotFound)?;
    let binding = assignment
        .binding
        .entries
        .iter()
        .find(|entry| entry.requirement_key == requirement_key)
        .ok_or(AgentFailure::NotFound)?;
    Ok(BindingContext {
        assignment_id,
        requirement_key: requirement_key.into(),
        capability: requirement.capability.clone(),
        contract_version: requirement.contract_version,
        selected: binding.selected.clone(),
        binding_revision: assignment.binding.revision,
        package: installation.package.clone(),
        definition_revision: manifest.definition.definition_revision,
    })
}

fn candidate_catalog(
    context: &BindingContext,
    live: &[floe_context::SourceCandidate],
) -> Result<crate::ExpertCandidateCatalog, AgentFailure> {
    let mut candidates = live
        .iter()
        .map(|candidate| crate::ExpertSourceCandidateView {
            candidate_id: candidate.candidate_id.clone(),
            title: candidate.title.clone(),
            detail: candidate.detail.clone(),
            availability: "available".into(),
            selected: context.selected.contains(&candidate.reference),
        })
        .collect::<Vec<_>>();
    for selected in &context.selected {
        if live
            .iter()
            .any(|candidate| candidate.reference == *selected)
        {
            continue;
        }
        candidates.push(crate::ExpertSourceCandidateView {
            candidate_id: floe_context::source_candidate_id(selected)?,
            title: "Saved source".into(),
            detail: "Unavailable; choose another source or remove it".into(),
            availability: "unavailable".into(),
            selected: true,
        });
    }
    Ok(crate::ExpertCandidateCatalog {
        assignment_id: context.assignment_id,
        requirement_key: context.requirement_key.clone(),
        binding_revision: context.binding_revision,
        candidates,
    })
}

async fn discover_live_candidates<Keys: floe_vault::VaultKeyProvider>(
    open: &OpenVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    context: &BindingContext,
    cancellation: &floe_execution::Cancellation,
) -> Result<Vec<floe_context::SourceCandidate>, AgentFailure> {
    let source_connectors: &[&str] = match context.capability.as_str() {
        "calendar.timeline" => &[
            "calendar.event_kit",
            "calendar.google",
            "calendar.microsoft",
            "calendar.fixture",
        ],
        "people.identity" => &["contacts.apple", "contacts.android"],
        "attention.coarse" => &["attention.macos"],
        "wellbeing.derived" => &["health.apple"],
        _ => &[],
    };
    let mut source_connections = Vec::new();
    for connector in source_connectors {
        let connector = floe_context_contract::ConnectorId::try_new(*connector)
            .map_err(|_| AgentFailure::InvalidInput)?;
        source_connections.extend(
            open.core
                .source_service()
                .list_current(person_id, &connector)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?,
        );
    }
    let remote = if matches!(
        context.capability.as_str(),
        "mail.communication" | "work.context" | "life.logistics"
    ) {
        if let Some(client) =
            floe_provider_adapters::sources::ServerSourceClient::from_current_connection(
                &open.connections,
                &person_id.to_string(),
                device_id,
            )?
        {
            let producer = open.vault.remote_pinned_producer().await?;
            (
                client
                    .observe_source_connections(
                        tokio::time::Instant::now() + std::time::Duration::from_secs(10),
                        cancellation,
                    )
                    .await?,
                Some(producer.execution_owner),
            )
        } else {
            (Vec::new(), None)
        }
    } else {
        (Vec::new(), None)
    };
    let remote_execution_owner = if context.capability == "calendar.timeline"
        && source_connections
            .iter()
            .any(|connection| connection.connector_id().as_str() != "calendar.event_kit")
    {
        floe_provider_adapters::sources::ServerSourceClient::from_current_connection(
            &open.connections,
            &person_id.to_string(),
            device_id,
        )?
        .ok_or(AgentFailure::CapabilityUnavailable)?;
        Some(open.vault.remote_pinned_producer().await?.execution_owner)
    } else {
        remote.1
    };
    floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id,
        device_id,
        capability: &context.capability,
        contract_version: context.contract_version,
        remote_connections: &remote.0,
        remote_execution_owner: remote_execution_owner.as_deref(),
        source_connections: &source_connections,
    })
}

async fn current_candidates<Keys: floe_vault::VaultKeyProvider>(
    open: &OpenVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    assignment_id: Uuid,
    requirement_key: &str,
    cancellation: &floe_execution::Cancellation,
) -> Result<CurrentCandidates, AgentFailure> {
    let context =
        binding_context(open, person_id, device_id, assignment_id, requirement_key).await?;
    let live =
        match discover_live_candidates(open, person_id, device_id, &context, cancellation).await {
            Ok(live) => live,
            Err(failure)
                if !context.selected.is_empty()
                    && matches!(
                        failure,
                        AgentFailure::CapabilityUnavailable
                            | AgentFailure::StorageUnavailable
                            | AgentFailure::VaultUnavailable
                            | AgentFailure::ServerModelUnavailable
                            | AgentFailure::ServerModelTimeout
                            | AgentFailure::DeadlineExceeded
                    ) =>
            {
                Vec::new()
            }
            Err(failure) => return Err(failure),
        };
    Ok(CurrentCandidates {
        catalog: candidate_catalog(&context, &live)?,
        live,
        package: context.package,
        definition_revision: context.definition_revision,
    })
}

pub(super) async fn inspect<Keys: floe_vault::VaultKeyProvider>(
    open: &OpenVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    assignment_id: Uuid,
    requirement_key: &str,
    cancellation: &floe_execution::Cancellation,
) -> Result<crate::ExpertCandidateCatalog, AgentFailure> {
    Ok(current_candidates(
        open,
        person_id,
        device_id,
        assignment_id,
        requirement_key,
        cancellation,
    )
    .await?
    .catalog)
}

pub(super) async fn bind_initial_defaults<Keys: floe_vault::VaultKeyProvider + 'static>(
    open: &OpenVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    setup_operation_id: Uuid,
    cancellation: &floe_execution::Cancellation,
) -> Result<(), AgentFailure> {
    if setup_operation_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    let snapshot = open
        .vault
        .expert_registry()
        .await?
        .ok_or(AgentFailure::NotFound)?;
    for assignment in snapshot
        .assignments
        .iter()
        .filter(|entry| entry.person_id == person_id)
    {
        let Some(installation) = snapshot
            .installations
            .iter()
            .find(|entry| entry.id == assignment.installation_id)
        else {
            return Err(AgentFailure::StorageUnavailable);
        };
        let Some(manifest) = snapshot
            .manifests
            .iter()
            .find(|entry| entry.package == installation.package)
        else {
            return Err(AgentFailure::StorageUnavailable);
        };
        if !floe_experts_builtin::manifests()
            .iter()
            .any(|shipped| shipped.package == manifest.package)
        {
            continue;
        }
        for requirement in &manifest.source_requirements {
            let current = match current_candidates(
                open,
                person_id,
                device_id,
                assignment.id,
                &requirement.key,
                cancellation,
            )
            .await
            {
                Ok(current) => current,
                Err(
                    failure @ (AgentFailure::Cancelled
                    | AgentFailure::VaultUnavailable
                    | AgentFailure::StorageUnavailable),
                ) => return Err(failure),
                Err(_) => continue,
            };
            if current
                .catalog
                .candidates
                .iter()
                .any(|candidate| candidate.selected)
            {
                continue;
            }
            let chosen = match requirement.capability.as_str() {
                "calendar.timeline" if !current.live.is_empty() => {
                    current.live.iter().collect::<Vec<_>>()
                }
                "relationships.confirmed_interactions" => Vec::new(),
                _ if current.live.len() == 1 => current.live.iter().collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            if chosen.is_empty() || chosen.len() > usize::from(requirement.maximum_sources) {
                continue;
            }
            let selected = chosen
                .into_iter()
                .map(|candidate| candidate.reference.clone())
                .collect();
            let operation_id = Uuid::new_v5(
                &setup_operation_id,
                format!(
                    "floe.initial-expert-binding.v1:{}:{}",
                    assignment.id, requirement.key
                )
                .as_bytes(),
            );
            match open
                .vault
                .replace_expert_binding(
                    operation_id,
                    floe_experts::ExpertBindingCommand {
                        assignment_id: assignment.id,
                        package: installation.package.clone(),
                        definition_revision: manifest.definition.definition_revision,
                        requirement_key: requirement.key.clone(),
                        expected_binding_revision: current.catalog.binding_revision,
                        selected,
                    },
                )
                .await
            {
                Ok(_) | Err(AgentFailure::Conflict) => {}
                Err(failure) => return Err(failure),
            }
        }
    }
    Ok(())
}

pub(super) async fn replace<Keys: floe_vault::VaultKeyProvider + 'static>(
    open: &OpenVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    operation_id: Uuid,
    change: &crate::ExpertBindingSelectionIntent,
    cancellation: &floe_execution::Cancellation,
) -> Result<crate::ExpertCandidateCatalog, AgentFailure> {
    if operation_id.is_nil()
        || change.candidate_ids.len() > usize::from(floe_experts::MAX_REQUIREMENT_SOURCES)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let context = binding_context(
        open,
        person_id,
        device_id,
        change.assignment_id,
        &change.requirement_key,
    )
    .await?;
    if context.package.id != change.package_id
        || context.package.version != change.package_version
        || context.package.kind != PackageKind::Expert
        || context.definition_revision != change.definition_revision
    {
        return Err(AgentFailure::Conflict);
    }
    if let Some(snapshot) = open.vault.expert_registry().await?
        && let Some(assignment) = snapshot
            .assignments
            .iter()
            .find(|entry| entry.id == change.assignment_id && entry.person_id == person_id)
        && assignment
            .binding
            .last_operation
            .as_ref()
            .is_some_and(|receipt| receipt.operation_id == operation_id)
    {
        let entry = assignment
            .binding
            .entries
            .iter()
            .find(|entry| entry.requirement_key == change.requirement_key)
            .ok_or(AgentFailure::Conflict)?;
        let mut requested = change.candidate_ids.clone();
        requested.sort();
        if requested.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AgentFailure::InvalidInput);
        }
        let mut recorded = entry
            .selected
            .iter()
            .map(floe_context::source_candidate_id)
            .collect::<Result<Vec<_>, _>>()?;
        recorded.sort();
        if requested != recorded {
            return Err(AgentFailure::Conflict);
        }
        open.vault
            .replace_expert_binding(
                operation_id,
                floe_experts::ExpertBindingCommand {
                    assignment_id: change.assignment_id,
                    package: floe_agent_contract::PackageRef {
                        id: change.package_id.clone(),
                        version: change.package_version.clone(),
                        kind: PackageKind::Expert,
                    },
                    definition_revision: change.definition_revision,
                    requirement_key: change.requirement_key.clone(),
                    expected_binding_revision: change.expected_binding_revision,
                    selected: entry.selected.clone(),
                },
            )
            .await?;
        if requested.is_empty() {
            return candidate_catalog(&context, &[]);
        }
        return Ok(current_candidates(
            open,
            person_id,
            device_id,
            change.assignment_id,
            &change.requirement_key,
            cancellation,
        )
        .await?
        .catalog);
    }
    if change.candidate_ids.is_empty() {
        open.vault
            .replace_expert_binding(
                operation_id,
                floe_experts::ExpertBindingCommand {
                    assignment_id: change.assignment_id,
                    package: context.package,
                    definition_revision: change.definition_revision,
                    requirement_key: change.requirement_key.clone(),
                    expected_binding_revision: change.expected_binding_revision,
                    selected: Vec::new(),
                },
            )
            .await?;
        open.publish_expert_directory(&open.registrations).await?;
        let updated = binding_context(
            open,
            person_id,
            device_id,
            change.assignment_id,
            &change.requirement_key,
        )
        .await?;
        return candidate_catalog(&updated, &[]);
    }
    let current = current_candidates(
        open,
        person_id,
        device_id,
        change.assignment_id,
        &change.requirement_key,
        cancellation,
    )
    .await?;
    if current.package.id != change.package_id
        || current.package.version != change.package_version
        || current.package.kind != PackageKind::Expert
        || current.definition_revision != change.definition_revision
    {
        return Err(AgentFailure::Conflict);
    }
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for candidate_id in &change.candidate_ids {
        if candidate_id.len() != 64
            || !candidate_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !seen.insert(candidate_id)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let candidate = current
            .live
            .iter()
            .find(|candidate| candidate.candidate_id == *candidate_id)
            .ok_or(AgentFailure::Conflict)?;
        selected.push(candidate.reference.clone());
    }
    selected.sort();
    let binding = open
        .vault
        .replace_expert_binding(
            operation_id,
            floe_experts::ExpertBindingCommand {
                assignment_id: change.assignment_id,
                package: current.package,
                definition_revision: change.definition_revision,
                requirement_key: change.requirement_key.clone(),
                expected_binding_revision: change.expected_binding_revision,
                selected,
            },
        )
        .await?;
    open.publish_expert_directory(&open.registrations).await?;
    let mut catalog = current.catalog;
    catalog.binding_revision = binding.revision;
    for candidate in &mut catalog.candidates {
        candidate.selected = change.candidate_ids.contains(&candidate.candidate_id);
    }
    Ok(catalog)
}
