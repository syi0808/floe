//! Current source candidates for an immutable Experts binding review.
use std::{collections::BTreeSet, sync::Arc};

use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionScope};
use floe_context_contract::{ConnectorId, SourceSelectionReference};
use floe_experts::{Candidate, CandidateAvailability, CandidateCatalog, CandidateQuery,
    CandidateSnapshot, CandidateSourceExpectation};
use sha2::{Digest, Sha256};

use super::expert_execution::{ExpertContextDependencies, check_scope};

pub struct ContextCandidateCatalog { dependencies: Arc<ExpertContextDependencies> }

impl ContextCandidateCatalog {
    pub fn new(dependencies: Arc<ExpertContextDependencies>) -> Result<Self, AgentFailure> {
        dependencies.validate()?;
        Ok(Self { dependencies })
    }
}

impl CandidateCatalog for ContextCandidateCatalog {
    fn inspect<'a>(&'a self, query: CandidateQuery, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<CandidateSnapshot, AgentFailure>>
    {
        Box::pin(async move {
            let dependencies = self.dependencies.as_ref();
            dependencies.authorize_actor(&query.actor)?;
            check_scope(scope)?;
            let manifest = dependencies.manifests.iter().find(|manifest|
                manifest.package == query.package_ref
                    && manifest.definition.definition_revision == query.definition_revision)
                .ok_or(AgentFailure::CapabilityDenied)?;
            let requirement = manifest.source_requirements.iter().find(|requirement|
                requirement.key == query.requirement_key).ok_or(AgentFailure::CapabilityDenied)?;
            if query.current_candidate_refs.len() > usize::from(requirement.maximum_sources) {
                return Err(AgentFailure::InvalidInput);
            }
            let mut selected = BTreeSet::new();
            for reference in &query.current_candidate_refs {
                reference.validate().map_err(|_| AgentFailure::InvalidInput)?;
                if reference.capability_id != requirement.capability
                    || reference.contract_version != requirement.contract_version
                    || !selected.insert(reference)
                { return Err(AgentFailure::InvalidInput); }
            }
            let connectors: &[&str] = match requirement.capability.as_str() {
                "calendar.timeline" => &["calendar.event_kit", "calendar.google", "calendar.microsoft", "calendar.fixture"],
                "people.identity" => &["contacts.apple", "contacts.android"],
                "attention.coarse" => &["attention.macos"],
                "wellbeing.derived" => &["health.apple"],
                _ => &[],
            };
            let mut sources = Vec::new();
            for connector in connectors {
                let connector = ConnectorId::try_new(*connector).map_err(|_| AgentFailure::InvalidInput)?;
                let rows = scope.run(async { dependencies.connections.list_current(query.actor.person_id, &connector)
                    .await.map_err(|_| AgentFailure::StorageUnavailable) }).await?;
                if rows.iter().any(|row| row.person_id() != query.actor.person_id
                    || row.connector_id() != &connector) { return Err(AgentFailure::StorageUnavailable); }
                sources.extend(rows);
            }
            let requires_remote = matches!(requirement.capability.as_str(),
                "mail.communication" | "work.context" | "life.logistics")
                || requirement.capability == "calendar.timeline" && sources.iter().any(|source|
                    source.connector_id().as_str() != "calendar.event_kit");
            let remote = if requires_remote {
                scope.run(dependencies.transport.remote(&query.actor, scope)).await?
            } else { None };
            if let Some(remote) = &remote {
                let observed = scope.run(remote.transport.producer_identity(&floe_access::RemoteCallWindow {
                    deadline: scope.deadline(), cancellation: scope.cancellation().clone(),
                })).await?;
                floe_access::producer_is_pinned(remote.transport.producer(), &observed)?;
            }
            let remote_catalog = match &remote {
                Some(remote) if matches!(requirement.capability.as_str(),
                    "mail.communication" | "work.context" | "life.logistics") => {
                    let mut catalog = scope.run(remote.transport.catalog(scope)).await?;
                    if catalog.len() > 256 { return Err(AgentFailure::BudgetExceeded); }
                    catalog.sort_by(|left, right| left.connection.connector_id.cmp(&right.connection.connector_id)
                        .then(left.connection.connection_id.cmp(&right.connection.connection_id)));
                    if catalog.windows(2).any(|pair| pair[0].connection.connector_id == pair[1].connection.connector_id
                        && pair[0].connection.connection_id == pair[1].connection.connection_id)
                    { return Err(AgentFailure::Conflict); }
                    catalog
                }
                _ => Vec::new(),
            };
            let mut live = crate::discover_source_candidates(crate::SourceCandidateRequest {
                person_id: query.actor.person_id, device_id: &query.actor.device_id,
                capability: &requirement.capability, contract_version: requirement.contract_version,
                remote_connections: &remote_catalog,
                remote_execution_owner: remote.as_ref().map(|remote| remote.transport.producer().execution_owner.as_str()),
                source_connections: &sources,
            })?;
            if live.len() + query.current_candidate_refs.len() > 256 {
                return Err(AgentFailure::BudgetExceeded);
            }
            let mut expectations = Vec::new();
            let mut candidates = Vec::new();
            for candidate in live.drain(..) {
                if candidate.reference.connector_id.as_str() != crate::LOCAL_CONTEXT_CONNECTOR {
                    let row = scope.run(async {
                        dependencies.connections.load(query.actor.person_id, &candidate.reference.connection_id)
                            .await.map_err(|_| AgentFailure::StorageUnavailable)
                    }).await?;
                    let Some(row) = row else { continue; };
                    if !matches_source(&candidate.reference, &row, query.actor.person_id)
                        || !row.is_serving() { continue; }
                    let fenced = scope.run(async {
                        dependencies.connections.source_is_fenced(query.actor.person_id, row.connection_id())
                            .await.map_err(|_| AgentFailure::StorageUnavailable)
                    }).await?;
                    if fenced { continue; }
                    expectations.push(CandidateSourceExpectation { reference: candidate.reference.clone(),
                        source_revision: row.revision(), source_authority: row.source_authority() });
                } else {
                    crate::validate_local_source_selection(&candidate.reference, &query.actor.device_id)?;
                }
                candidates.push(Candidate { candidate_id: candidate.candidate_id, label: candidate.title,
                    detail: candidate.detail, availability: CandidateAvailability::Available,
                    reference: candidate.reference });
            }
            for reference in &query.current_candidate_refs {
                if candidates.iter().any(|candidate| &candidate.reference == reference) { continue; }
                candidates.push(Candidate { candidate_id: crate::source_candidate_id(reference)?,
                    label: "Saved source".into(), detail: "Unavailable; choose another source or remove it".into(),
                    availability: CandidateAvailability::Unavailable, reference: reference.clone() });
                if reference.connector_id.as_str() != crate::LOCAL_CONTEXT_CONNECTOR {
                    if let Some(row) = scope.run(async {
                        dependencies.connections.load(query.actor.person_id, &reference.connection_id)
                            .await.map_err(|_| AgentFailure::StorageUnavailable)
                    }).await? {
                        if matches_source(reference, &row, query.actor.person_id) {
                            expectations.push(CandidateSourceExpectation { reference: reference.clone(),
                                source_revision: row.revision(), source_authority: row.source_authority() });
                        }
                    }
                }
            }
            candidates.sort_by(|left, right| left.reference.cmp(&right.reference));
            expectations.sort_by(|left, right| left.reference.cmp(&right.reference));
            if candidates.len() > 64 { return Err(AgentFailure::BudgetExceeded); }
            if candidates.windows(2).any(|pair| pair[0].reference == pair[1].reference
                || pair[0].candidate_id == pair[1].candidate_id)
                || expectations.windows(2).any(|pair| pair[0].reference == pair[1].reference)
            { return Err(AgentFailure::Conflict); }
            // This is the pinned catalog contract revision. The full digest
            // identifies its current composite source/Gateway observations.
            let revision = manifest.definition.definition_revision;
            let producer = remote.as_ref().map(|remote|
                (remote.transport.client_id(), remote.transport.producer()));
            // Poll timestamps, item counts and current view contents do not
            // change which source can be selected. Binding replacement must
            // compare durable catalog facts, not the wall time of two reads.
            let remote_identity = remote_catalog.iter().map(|snapshot| (
                &snapshot.descriptor, &snapshot.connection.connector_id,
                &snapshot.connection.connection_id, &snapshot.connection.person_id,
                &snapshot.connection.device_binding, &snapshot.connection.state,
                &snapshot.connection.granted_scopes,
            )).collect::<Vec<_>>();
            let bytes = serde_json::to_vec(&("floe.expert-candidate-catalog.sha256.v1",
                query.actor.person_id, &query.actor.device_id, &manifest.package, revision, requirement, &candidates, &expectations,
                producer, remote_identity)).map_err(|_| AgentFailure::InvalidInput)?;
            if bytes.len() > crate::MAX_LEASE_BYTES { return Err(AgentFailure::BudgetExceeded); }
            check_scope(scope)?;
            Ok(CandidateSnapshot { revision, digest: Sha256::digest(bytes).into(),
                candidates, source_expectations: expectations })
        })
    }
}

fn matches_source(reference: &SourceSelectionReference, source: &floe_connections::SourceConnection,
    person: floe_kernel::PersonId) -> bool
{
    source.person_id() == person && source.connector_id() == &reference.connector_id
        && source.connection_id() == &reference.connection_id
        && source.execution_owner_id() == &reference.execution_owner_id
        && source.revision() > 0 && source.source_authority().is_valid()
}
