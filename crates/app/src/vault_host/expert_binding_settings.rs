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

async fn current_candidates<Keys: floe_vault::VaultKeyProvider>(
    open: &OpenVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    assignment_id: Uuid,
    requirement_key: &str,
    cancellation: &floe_execution::Cancellation,
) -> Result<CurrentCandidates, AgentFailure> {
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
    let calendar_connection = if requirement.capability == "calendar.timeline" {
        open.core
            .calendar_connection(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
    } else {
        None
    };
    let remote = if matches!(
        requirement.capability.as_str(),
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
    let live = floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
        person_id,
        device_id,
        capability: &requirement.capability,
        contract_version: requirement.contract_version,
        remote_connections: &remote.0,
        remote_execution_owner: remote.1.as_deref(),
        calendar_connection: calendar_connection.as_ref(),
    })?;
    let mut candidates = live
        .iter()
        .map(|candidate| crate::ExpertSourceCandidateView {
            candidate_id: candidate.candidate_id.clone(),
            title: candidate.title.clone(),
            detail: candidate.detail.clone(),
            availability: "available".into(),
            selected: binding.selected.contains(&candidate.reference),
        })
        .collect::<Vec<_>>();
    for selected in &binding.selected {
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
    Ok(CurrentCandidates {
        catalog: crate::ExpertCandidateCatalog {
            assignment_id,
            requirement_key: requirement_key.into(),
            binding_revision: assignment.binding.revision,
            candidates,
        },
        live,
        package: installation.package.clone(),
        definition_revision: manifest.definition.definition_revision,
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
