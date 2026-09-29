//! Host owner reads and mutations for interaction decisions.
//!
//! [`HostInteractionOwners`] implements the [`ObserveStateReader`] and
//! [`InlineOwnerMutation`] seams of [`super::interaction_resolution`] against
//! the real owners: core connections, vault grants and policies, live
//! native subjects, the pinned remote producer, and the canonical owner
//! operations (native/personal `Review`, remote `ConnectionObserve`
//! enable).
//!
//! Decision-time reads are local owner truth, comparable field by field
//! with the immutable reviewed target. Anything unprovable locally fails
//! the decision closed; nothing here manufactures replacement expectations.
//! The remote enable path additionally re-reads through `review_bundle`
//! and compares the fresh bundle with the reviewed target before calling
//! `enable_bundle`, because provider identity and recipient are live
//! routing facts the publication-time capture cannot bind. They are
//! verified live inside the enable; every authority field must still equal
//! the reviewed values.

use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_context_contract::SourceAuthority;
use floe_inference::SavedConnectionStore;
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

use super::interaction_resolution::{
    InlineOwnerMutation, LiveGrant, LiveInlineState, LiveMember, ObserveStateReader,
    RecipientConsentOwner,
};

fn native_source_scope(
    connection: &floe_connections::SourceConnection,
) -> floe_context_contract::CalendarScope {
    match connection.resource_mode() {
        floe_connections::ResourceMode::Selected => floe_context_contract::CalendarScope::Selected,
        floe_connections::ResourceMode::AllAvailable => floe_context_contract::CalendarScope::All,
    }
}

fn native_inline_operation(
    target: &floe_conversation::InlineObserveTarget,
    connector_id: &str,
) -> Result<crate::ConnectionObserveOperation, AgentFailure> {
    let source_authority = target
        .source_revision
        .as_ref()
        .and_then(|revision| {
            std::num::NonZeroU64::new(revision.epoch)
                .and_then(|epoch| SourceAuthority::from_parts(revision.incarnation, epoch))
        })
        .ok_or(AgentFailure::InvalidInput)?;
    let members = target
        .members
        .iter()
        .map(|member| {
            let (expected_grant_id, expected_grant_authority) = match &member.expected_grant {
                floe_conversation::ExpectedGrantState::Absent => (None, None),
                floe_conversation::ExpectedGrantState::Active {
                    grant_id,
                    authority_incarnation,
                    authority_epoch,
                } => {
                    let id = floe_access::GrantId::from_uuid(*grant_id)
                        .ok_or(AgentFailure::InvalidInput)?;
                    let authority = std::num::NonZeroU64::new(*authority_epoch)
                        .and_then(|epoch| {
                            floe_access::GrantAuthority::from_parts(*authority_incarnation, epoch)
                        })
                        .ok_or(AgentFailure::InvalidInput)?;
                    (Some(id), Some(authority))
                }
            };
            Ok(crate::ConnectionObserveReviewedMember {
                view_id: member.member_id.clone(),
                policy_digest: member.policy_digest.clone(),
                resource: member.resource.clone(),
                expected_grant_id,
                expected_grant_authority,
            })
        })
        .collect::<Result<Vec<_>, AgentFailure>>()?;
    let expectation = crate::ConnectionObserveExpectation {
        connector_id: connector_id.to_owned(),
        connection_id: target.connection_id.clone(),
        source_authority,
        connection_revision: target.connection_revision,
        native_subject: target.reviewed_native_subject.clone(),
        producer_fingerprint: None,
        members,
    };
    expectation.validate()?;
    Ok(crate::ConnectionObserveOperation::SetEnabled {
        connector_id: connector_id.to_owned(),
        connection_id: target.connection_id.clone(),
        enabled: true,
        disconnecting: false,
        expected: Some(expectation),
    })
}

/// The host's owner access for interaction decisions: core connections,
/// vault grants, saved-connection transports, and device subject probes.
pub(crate) struct HostInteractionOwners<'a, Keys, CalendarSubject, PersonalInspector>
where
    Keys: VaultKeyProvider,
{
    pub core: &'a crate::FloeCore,
    pub vault: &'a EncryptedAgentVault<Keys>,
    pub connections: &'a floe_provider_adapters::control::CurrentSavedConnectionStore,
    pub calendar_subject: &'a CalendarSubject,
    pub personal_subject: &'a PersonalInspector,
    /// Decision-time probes stay bounded: reads run under this deadline,
    /// not the turn's.
    pub probe_deadline: tokio::time::Instant,
}

impl<Keys, CalendarSubject, PersonalInspector> ObserveStateReader
    for HostInteractionOwners<'_, Keys, CalendarSubject, PersonalInspector>
where
    Keys: VaultKeyProvider,
    CalendarSubject: floe_context::NativeCalendarSubjectSource,
    PersonalInspector: floe_access::PersonalSubjectInspector,
{
    fn expert_review_current<'a>(
        &'a self,
        interaction: &'a floe_conversation::ConversationInteraction,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            let floe_conversation::InteractionOrigin::Task { task_id, .. } = interaction.origin
            else {
                return Ok(true);
            };
            let floe_conversation::ReviewedTarget::InlineObserve(target) = &interaction.target
            else {
                return Ok(true);
            };
            let task_id = floe_agent_contract::TaskId::from_uuid(task_id)
                .ok_or(AgentFailure::InvalidInput)?;
            let Some(task) = self.vault.task(task_id).await? else {
                return Ok(false);
            };
            if task.snapshot.principal != interaction.person_id.to_string()
                || task.admission.package.id != target.consumer
            {
                return Ok(false);
            }
            let snapshot = self
                .vault
                .expert_registry()
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let registry =
                floe_experts::AgentRegistry::restore(snapshot, self.vault.registry_instance_id())?;
            if registry
                .validate_current_execution_selection(
                    interaction.person_id,
                    &task.admission,
                    &task.selection,
                    true,
                )
                .is_err()
            {
                return Ok(false);
            }
            let Some(connector) = target.connector_id.as_deref() else {
                return Ok(false);
            };
            Ok(reviewed_target_matches_selection(
                target,
                connector,
                &task.selection,
            ))
        })
    }

    fn read_live_inline<'a>(
        &'a self,
        target: &'a floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<LiveInlineState, AgentFailure>> {
        Box::pin(async move {
            self.read_inline(target, person_id, device_id, cancellation)
                .await
        })
    }

    fn navigation_connection_usable<'a>(
        &'a self,
        target: &'a floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move { self.nav_connection_usable(target, person_id).await })
    }

    fn navigation_satisfied<'a>(
        &'a self,
        target: &'a floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            self.nav_satisfied(target, person_id, device_id, cancellation)
                .await
        })
    }
}

fn reviewed_target_matches_selection(
    target: &floe_conversation::InlineObserveTarget,
    connector: &str,
    selection: &floe_experts::ExpertExecutionSelection,
) -> bool {
    target.members.iter().all(|member| {
        selection.requirements.iter().any(|requirement| {
            requirement.selected.iter().any(|selected| {
                selected.connector_id.as_str() == connector
                    && selected.connection_id.as_str() == target.connection_id
                    && selected.resource.as_str() == member.resource
            })
        })
    })
}

impl<Keys, CalendarSubject, PersonalInspector> InlineOwnerMutation
    for HostInteractionOwners<'_, Keys, CalendarSubject, PersonalInspector>
where
    Keys: VaultKeyProvider,
    CalendarSubject: floe_context::NativeCalendarSubjectSource,
    PersonalInspector: floe_access::PersonalSubjectInspector,
{
    fn enable_reviewed<'a>(
        &'a self,
        target: &'a floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.enable(target, person_id, device_id, cancellation)
                .await
        })
    }
}

impl<Keys, CalendarSubject, PersonalInspector> RecipientConsentOwner
    for HostInteractionOwners<'_, Keys, CalendarSubject, PersonalInspector>
where
    Keys: VaultKeyProvider,
    CalendarSubject: floe_context::NativeCalendarSubjectSource,
    PersonalInspector: floe_access::PersonalSubjectInspector,
{
    fn grant_reviewed<'a>(
        &'a self,
        target: &'a floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &'a str,
        now_unix_ms: i64,
    ) -> BoxFuture<'a, Result<floe_access::RecipientConsent, AgentFailure>> {
        Box::pin(async move {
            self.grant_consent(target, person_id, device_id, now_unix_ms)
                .await
        })
    }

    fn usable_consent<'a>(
        &'a self,
        target: &'a floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &'a str,
        now_unix_ms: i64,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            self.consent_usable(target, person_id, device_id, now_unix_ms)
                .await
        })
    }
}

impl<Keys, CalendarSubject, PersonalInspector>
    HostInteractionOwners<'_, Keys, CalendarSubject, PersonalInspector>
where
    Keys: VaultKeyProvider,
    CalendarSubject: floe_context::NativeCalendarSubjectSource,
    PersonalInspector: floe_access::PersonalSubjectInspector,
{
    async fn read_inline(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<LiveInlineState, AgentFailure> {
        if self.vault.person_id() != person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        target.validate().map_err(|_| AgentFailure::InvalidInput)?;
        let connector = target
            .connector_id
            .as_deref()
            .ok_or(AgentFailure::InvalidInput)?;
        if floe_access::native_calendar_provider(connector).is_some() {
            return self
                .read_native_calendar(target, connector, person_id, device_id, cancellation)
                .await;
        }
        if matches!(
            connector,
            floe_access::ATTENTION_CONNECTOR | floe_access::WELLBEING_CONNECTOR
        ) {
            return self
                .read_personal(target, connector, person_id, device_id, cancellation)
                .await;
        }
        if !crate::first_party_observe::remote_policies(connector)?.is_empty() {
            return self.read_remote(target, connector, person_id).await;
        }
        Err(AgentFailure::CapabilityUnavailable)
    }

    /// The definitively unbound state: no usable connection, no members.
    /// Callers supersede against this; it never looks satisfied.
    fn unusable() -> LiveInlineState {
        LiveInlineState {
            source_revision: None,
            members: Vec::new(),
            connection_revision: None,
            producer_fingerprint: None,
            native_subject: None,
            connection_usable: false,
        }
    }

    async fn read_native_calendar(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        connector: &str,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<LiveInlineState, AgentFailure> {
        let live = floe_context::CalendarConnectionReader::calendar_connection(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
        )
        .await?;
        let Some(connection) = live else {
            return Ok(Self::unusable());
        };
        if connection.connection_id().as_str() != target.connection_id
            || connection.execution_owner_id().as_str() != device_id
            || connection.state() == floe_connections::SourceState::Disconnected
            || !connection.source_authority().is_valid()
        {
            return Ok(Self::unusable());
        }
        let logical_resource = floe_access::native_calendar_resource(&target.connection_id)?;
        if target.members.len() != 1
            || target.members[0].member_id != "calendar.timeline"
            || target.members[0].resource != logical_resource.as_str()
        {
            return Ok(Self::unusable());
        }
        let calendar_ids: Vec<String> = connection
            .resources()
            .iter()
            .map(|calendar| calendar.handle().as_str().to_owned())
            .collect();
        let observation = floe_context::preview_native_calendar_subject(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
            self.calendar_subject,
            &floe_context::NativeCalendarSourceRequest {
                person_id,
                provider: floe_context_contract::CalendarProvider::EventKit,
                device_id: device_id.to_owned(),
                calendar_ids,
                connection_scope: native_source_scope(&connection),
                source_authority: Some(connection.source_authority()),
                reviewed_native_subject_fingerprint: None,
                connection_id: Some(connection.connection_id().as_str().to_owned()),
            },
            &floe_access::RemoteCallWindow {
                deadline: self.probe_deadline,
                cancellation: cancellation.clone(),
            },
        )
        .await?;
        let policy_digest = crate::first_party_observe::policy_digest(
            &crate::first_party_observe::calendar_policy()?,
        )?;
        let member = &target.members[0];
        let mut live = self
            .probed_member(
                connector,
                &target.connection_id,
                &member.member_id,
                &member.resource,
                person_id,
                Some(device_id),
            )
            .await?;
        live.policy_digest = policy_digest;
        Ok(LiveInlineState {
            source_revision: Some(connection.source_authority()),
            members: vec![live],
            connection_revision: Some(connection.revision()),
            producer_fingerprint: None,
            native_subject: Some(observation.native_subject_fingerprint),
            connection_usable: true,
        })
    }

    async fn read_personal(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        connector: &str,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<LiveInlineState, AgentFailure> {
        let spec = crate::personal_source_spec::PersonalSourceSpec::for_connector(connector)?;
        let probe = match connector {
            floe_access::ATTENTION_CONNECTOR => floe_access::PersonalSubjectProbe::Attention,
            floe_access::WELLBEING_CONNECTOR => floe_access::PersonalSubjectProbe::Wellbeing,
            _ => return Err(AgentFailure::CapabilityUnavailable),
        };
        let connection_id = floe_context_contract::ConnectionId::try_new(&target.connection_id)
            .map_err(|_| AgentFailure::InvalidInput)?;
        let Some(source) = self
            .core
            .source_service()
            .load(person_id, &connection_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
        else {
            return Ok(Self::unusable());
        };
        if spec.validate_connection(&source, device_id).is_err() {
            return Ok(Self::unusable());
        }
        let logical =
            floe_context_contract::connection_view_resource(spec.view, source.connection_id())
                .map_err(|_| AgentFailure::InvalidInput)?;
        if target.members.len() != 1
            || target.members[0].member_id != spec.view
            || target.members[0].resource != logical.as_str()
        {
            return Ok(Self::unusable());
        }
        let subject = source
            .native_subject_fingerprint()
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let evidence = self
            .personal_subject
            .inspect(
                person_id,
                device_id,
                probe,
                Some(subject.to_owned()),
                Some(self.probe_deadline),
                cancellation.clone(),
            )
            .await?;
        if evidence.before != subject || evidence.after != subject {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let Some(current) = self
            .core
            .source_service()
            .load(person_id, source.connection_id())
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
        else {
            return Ok(Self::unusable());
        };
        if current.source_authority() != source.source_authority()
            || current.native_subject_fingerprint() != source.native_subject_fingerprint()
            || current
                .resources()
                .iter()
                .map(|resource| resource.handle())
                .ne(source.resources().iter().map(|resource| resource.handle()))
            || !current.is_serving()
        {
            return Ok(Self::unusable());
        }
        let member = self
            .probed_member(
                connector,
                source.connection_id().as_str(),
                spec.view,
                logical.as_str(),
                person_id,
                Some(device_id),
            )
            .await?;
        Ok(LiveInlineState {
            source_revision: Some(current.source_authority()),
            members: vec![member],
            connection_revision: Some(current.revision()),
            producer_fingerprint: None,
            native_subject: Some(evidence.before),
            connection_usable: true,
        })
    }

    async fn read_remote(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        connector: &str,
        person_id: PersonId,
    ) -> Result<LiveInlineState, AgentFailure> {
        let policies = crate::first_party_observe::remote_policies_for_target(
            self.vault,
            person_id,
            connector,
            &target.connection_id,
        )
        .await?;
        if policies.is_empty() {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let producer = match self.vault.remote_pinned_producer().await {
            Ok(producer) => producer,
            Err(_) => return Ok(Self::unusable()),
        };
        self.read_remote_views(target, connector, person_id, producer.fingerprint)
            .await
    }

    async fn read_remote_views(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        connector: &str,
        person_id: PersonId,
        producer_fingerprint: String,
    ) -> Result<LiveInlineState, AgentFailure> {
        let policies = crate::first_party_observe::remote_policies_for_target(
            self.vault,
            person_id,
            connector,
            &target.connection_id,
        )
        .await?;
        let connection_id = floe_context_contract::ConnectionId::try_new(&target.connection_id)
            .map_err(|_| AgentFailure::InvalidInput)?;
        let canonical: Vec<(String, String)> = policies
            .iter()
            .map(|policy| {
                Ok((
                    policy.view_id.to_owned(),
                    floe_context_contract::connection_view_resource(policy.view_id, &connection_id)
                        .map_err(|_| AgentFailure::InvalidInput)?
                        .as_str()
                        .to_owned(),
                ))
            })
            .collect::<Result<_, AgentFailure>>()?;
        // Reviewed keys outside the canonical set cannot bind.
        if !target.members.iter().all(|member| {
            canonical
                .iter()
                .any(|(view, resource)| *view == member.member_id && *resource == member.resource)
        }) {
            return Ok(Self::unusable());
        }
        let mut members = Vec::with_capacity(canonical.len());
        for member in &target.members {
            members.push(
                self.probed_member(
                    connector,
                    &target.connection_id,
                    &member.member_id,
                    &member.resource,
                    person_id,
                    None,
                )
                .await?,
            );
        }
        // Canonical members outside the reviewed set ride along so the
        // compare invalidates the review instead of widening it.
        for (view_id, resource) in &canonical {
            if target
                .members
                .iter()
                .any(|member| &member.member_id == view_id && &member.resource == resource)
            {
                continue;
            }
            members.push(
                self.probed_member(
                    connector,
                    &target.connection_id,
                    view_id,
                    resource,
                    person_id,
                    None,
                )
                .await?,
            );
        }
        Ok(LiveInlineState {
            source_revision: None,
            members,
            connection_revision: None,
            producer_fingerprint: Some(producer_fingerprint),
            native_subject: None,
            connection_usable: true,
        })
    }

    /// Probe one member key and its live grants with their source authority.
    #[allow(clippy::too_many_arguments)]
    async fn probed_member(
        &self,
        connector: &str,
        connection_id: &str,
        member_id: &str,
        resource: &str,
        person_id: PersonId,
        device_id: Option<&str>,
    ) -> Result<LiveMember, AgentFailure> {
        // Active only: a paused grant authorizes nothing, and capture's
        // sibling verification requires Active, so parity demands the same
        // filter here. A paused-where-reviewed-live member drifts instead
        // of settling.
        let grants = super::review_snapshot::live_grants_for_member(
            self.vault,
            person_id,
            connector,
            connection_id,
            resource,
        )
        .await?
        .into_iter()
        .filter(|grant| grant.state() == floe_access::GrantState::Active)
        .collect::<Vec<_>>();
        Ok(LiveMember {
            member_id: member_id.to_owned(),
            policy_digest: if connector == "calendar.event_kit" {
                crate::first_party_observe::policy_digest(
                    &crate::first_party_observe::calendar_policy()?,
                )?
            } else if let Some(device_id) = device_id {
                crate::first_party_observe::native_member_policy_digest_for_target(
                    self.vault, person_id, connector, device_id,
                )
                .await?
            } else if crate::first_party_observe::remote_policies(connector)?.is_empty() {
                crate::first_party_observe::member_policy_digest(connector, member_id)?
            } else {
                crate::first_party_observe::remote_member_policy_digest_for_target(
                    self.vault,
                    person_id,
                    connector,
                    connection_id,
                    member_id,
                )
                .await?
            },
            resource: resource.to_owned(),
            live_grants: grants
                .into_iter()
                .map(|grant| LiveGrant {
                    id: grant.id(),
                    authority: grant.authority(),
                })
                .collect(),
        })
    }

    /// Whether the navigation target's owning connection is definitively
    /// dead. Only a definitive answer supersedes; inconclusive reads keep
    /// the card actionable for a later retry.
    async fn nav_connection_usable(
        &self,
        target: &floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
    ) -> Result<bool, AgentFailure> {
        let Some(connection_id) = target.connection_id.as_deref() else {
            return Ok(true);
        };
        if let Ok(Some(live)) = floe_context::CalendarConnectionReader::calendar_connection(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
        )
        .await
        {
            if live.connection_id().as_str() == connection_id
                && live.state() != floe_connections::SourceState::Disconnected
            {
                return Ok(true);
            }
        }
        if let Ok(connection_id) = floe_context_contract::ConnectionId::try_new(connection_id) {
            if let Some(source) = self
                .core
                .source_service()
                .load(person_id, &connection_id)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?
            {
                if source.is_serving() {
                    return Ok(true);
                }
            }
        }
        match self.vault.remote_pinned_producer().await {
            Ok(_) => Ok(true),
            Err(AgentFailure::NotFound) => Ok(false),
            Err(_) => Ok(true),
        }
    }

    /// Whether the navigation requirement is already fully satisfied: the
    /// original read would now be admitted without any mutation. Native
    /// connections attribute by exact connection record; anything else
    /// attributes by the live grants' own connector and requires full
    /// canonical coverage there.
    async fn nav_satisfied(
        &self,
        target: &floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<bool, AgentFailure> {
        let Some(connection_id) = target.connection_id.as_deref() else {
            return Ok(false);
        };
        if self.vault.person_id() != person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        if let Ok(Some(live)) = floe_context::CalendarConnectionReader::calendar_connection(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
        )
        .await
        {
            if live.connection_id().as_str() == connection_id
                && live.execution_owner_id().as_str() == device_id
                && live.state() != floe_connections::SourceState::Disconnected
                && live.source_authority().is_valid()
            {
                return self
                    .native_scope_satisfied(&live, person_id, device_id, cancellation)
                    .await;
            }
        }
        if self
            .personal_scope_satisfied(connection_id, person_id, device_id, cancellation)
            .await?
        {
            return Ok(true);
        }
        self.remote_scope_satisfied(connection_id, person_id).await
    }

    /// Native satisfaction: one active logical View grant and a current
    /// subject probe over every Calendar resource.
    async fn native_scope_satisfied(
        &self,
        connection: &floe_connections::SourceConnection,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<bool, AgentFailure> {
        if connection.resources().is_empty() {
            return Ok(false);
        }
        let logical_resource =
            floe_access::native_calendar_resource(connection.connection_id().as_str())?;
        let covering = super::review_snapshot::live_grants_for_member(
            self.vault,
            person_id,
            "calendar.event_kit",
            connection.connection_id().as_str(),
            logical_resource.as_str(),
        )
        .await?;
        let [grant] = covering.as_slice() else {
            return Ok(false);
        };
        if grant.source().execution_owner() != connection.execution_owner_id()
            || grant.state() != floe_access::GrantState::Active
            || grant.review_required()
            || grant.scope().resources() != [logical_resource]
        {
            return Ok(false);
        }
        let calendar_ids: Vec<String> = connection
            .resources()
            .iter()
            .map(|calendar| calendar.handle().as_str().to_owned())
            .collect();
        let observation = floe_context::preview_native_calendar_subject(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
            self.calendar_subject,
            &floe_context::NativeCalendarSourceRequest {
                person_id,
                provider: floe_context_contract::CalendarProvider::EventKit,
                device_id: device_id.to_owned(),
                calendar_ids,
                connection_scope: native_source_scope(connection),
                source_authority: Some(connection.source_authority()),
                reviewed_native_subject_fingerprint: None,
                connection_id: Some(connection.connection_id().as_str().to_owned()),
            },
            &floe_access::RemoteCallWindow {
                deadline: self.probe_deadline,
                cancellation: cancellation.clone(),
            },
        )
        .await;
        Ok(observation.is_ok())
    }

    /// Personal satisfaction: exactly one live grant for a personal
    /// connector on this connection, with a stable subject probe.
    async fn personal_scope_satisfied(
        &self,
        connection_id: &str,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<bool, AgentFailure> {
        let connection_id = floe_context_contract::ConnectionId::try_new(connection_id)
            .map_err(|_| AgentFailure::InvalidInput)?;
        let Some(source) = self
            .core
            .source_service()
            .load(person_id, &connection_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
        else {
            return Ok(false);
        };
        let spec = match crate::personal_source_spec::PersonalSourceSpec::for_connector(
            source.connector_id().as_str(),
        ) {
            Ok(spec)
                if matches!(
                    spec.view,
                    floe_context_contract::ATTENTION_VIEW_ID
                        | floe_context_contract::WELLBEING_VIEW_ID
                ) =>
            {
                spec
            }
            _ => return Ok(false),
        };
        if spec.validate_connection(&source, device_id).is_err() {
            return Ok(false);
        }
        let logical =
            floe_context_contract::connection_view_resource(spec.view, source.connection_id())
                .map_err(|_| AgentFailure::InvalidInput)?;
        let grants = super::review_snapshot::live_grants_for_member(
            self.vault,
            person_id,
            spec.connector,
            source.connection_id().as_str(),
            logical.as_str(),
        )
        .await?;
        if !matches!(grants.as_slice(), [grant] if grant.state() == floe_access::GrantState::Active)
        {
            return Ok(false);
        }
        let probe = if spec.view == floe_context_contract::ATTENTION_VIEW_ID {
            floe_access::PersonalSubjectProbe::Attention
        } else {
            floe_access::PersonalSubjectProbe::Wellbeing
        };
        let Some(subject) = source.native_subject_fingerprint() else {
            return Ok(false);
        };
        let evidence = self
            .personal_subject
            .inspect(
                person_id,
                device_id,
                probe,
                Some(subject.to_owned()),
                Some(self.probe_deadline),
                cancellation.clone(),
            )
            .await;
        let Ok(evidence) = evidence else {
            return Ok(false);
        };
        if evidence.before != subject || evidence.after != subject {
            return Ok(false);
        }
        let current = self
            .core
            .source_service()
            .load(person_id, source.connection_id())
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        Ok(current.is_some_and(|current| {
            current.is_serving()
                && current.source_authority() == source.source_authority()
                && current.native_subject_fingerprint() == source.native_subject_fingerprint()
        }))
    }
    /// fails here and the resumed read re-authorizes anyway.
    async fn remote_scope_satisfied(
        &self,
        connection_id: &str,
        person_id: PersonId,
    ) -> Result<bool, AgentFailure> {
        if self.vault.remote_pinned_producer().await.is_err() {
            return Ok(false);
        }
        let mut connectors: Vec<String> = self
            .vault
            .list_data_access_grants(128)
            .await?
            .into_iter()
            .filter(|grant| {
                grant.source().person_id() == person_id
                    && grant.source().connection_id().as_str() == connection_id
                    && grant.state() != floe_access::GrantState::Revoked
            })
            .map(|grant| grant.source().connector().as_str().to_owned())
            .collect();
        connectors.sort();
        connectors.dedup();
        for connector in connectors {
            let policies = crate::first_party_observe::remote_policies(&connector)?;
            if policies.is_empty() {
                continue;
            }
            let mut covered = true;
            for policy in &policies {
                let resource = floe_context_contract::connection_view_resource(
                    policy.view_id,
                    &floe_context_contract::ConnectionId::try_new(connection_id)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                )
                .map_err(|_| AgentFailure::InvalidInput)?
                .as_str()
                .to_owned();
                let grants = super::review_snapshot::live_grants_for_member(
                    self.vault,
                    person_id,
                    &connector,
                    connection_id,
                    &resource,
                )
                .await?;
                if grants.len() != 1 {
                    covered = false;
                    break;
                }
            }
            if covered {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn enable(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<(), AgentFailure> {
        if self.vault.person_id() != person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        target.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if device_id.trim().is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let connector = target
            .connector_id
            .as_deref()
            .ok_or(AgentFailure::InvalidInput)?;
        if floe_access::native_calendar_provider(connector).is_some() {
            return self
                .enable_native_calendar(target, person_id, device_id, cancellation)
                .await;
        }
        if matches!(
            connector,
            floe_access::ATTENTION_CONNECTOR | floe_access::WELLBEING_CONNECTOR
        ) {
            return self
                .enable_personal(target, connector, person_id, device_id, cancellation)
                .await;
        }
        if !crate::first_party_observe::remote_policies(connector)?.is_empty() {
            return self
                .enable_remote(target, connector, person_id, device_id, cancellation)
                .await;
        }
        Err(AgentFailure::CapabilityUnavailable)
    }

    /// Grant the exact reviewed recipient consent, binding the live pairing.
    ///
    /// Admits the current saved connection against the verified caller for
    /// the pairing identity only; recorded global consent flags are never
    /// consulted. Without a live pairing there is no external dispatch to
    /// authorize, so the grant fails closed.
    async fn grant_consent(
        &self,
        target: &floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &str,
        now_unix_ms: i64,
    ) -> Result<floe_access::RecipientConsent, AgentFailure> {
        if self.vault.person_id() != person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        target.validate().map_err(|_| AgentFailure::InvalidInput)?;
        let now = chrono::DateTime::from_timestamp_millis(now_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let client_id = self.live_client_id(&person_id.to_string(), device_id)?;
        let consent = floe_access::RecipientConsent::try_new(
            person_id,
            device_id.to_owned(),
            client_id,
            target.recipient.clone(),
            target.profile_id.clone(),
            target.purpose.clone(),
            target.consumer.clone(),
            target.input_data_classes.clone(),
            target.source_scopes.clone(),
            target.lineage,
            target.projection_ref,
            target.projection_revision,
            now,
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        floe_access::grant_recipient_consent(self.vault, consent).await
    }

    /// Whether a usable consent already covers the reviewed content under
    /// the live pairing. Refresh-only: never grants.
    async fn consent_usable(
        &self,
        target: &floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &str,
        now_unix_ms: i64,
    ) -> Result<bool, AgentFailure> {
        if self.vault.person_id() != person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        target.validate().map_err(|_| AgentFailure::InvalidInput)?;
        let now = chrono::DateTime::from_timestamp_millis(now_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let Ok(client_id) = self.live_client_id(&person_id.to_string(), device_id) else {
            return Ok(false);
        };
        let id = floe_access::recipient_consent_id(
            person_id,
            device_id,
            &client_id,
            &target.recipient,
            &target.profile_id,
            &target.purpose,
            &target.consumer,
            &target.input_data_classes,
            &target.source_scopes,
            target.lineage,
        );
        let usable = floe_access::RecipientConsentStore::find_consent(self.vault, id)
            .await?
            .is_some_and(|consent| consent.is_usable_at(now));
        Ok(usable)
    }

    /// The live pairing identity for consent binding, admitted per call.
    ///
    /// Reloads the current saved connection and binds it to the verified
    /// person/device. Recorded global consent flags are accepted as stored
    /// shape but never consulted for authority.
    fn live_client_id(&self, person_id: &str, device_id: &str) -> Result<String, AgentFailure> {
        let stored = self
            .connections
            .load()
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let Some(saved) = stored else {
            return Err(AgentFailure::PolicyDenied);
        };
        let admitted = floe_inference::admit_saved_connection(saved, person_id, device_id)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        Ok(admitted.client_id)
    }

    async fn enable_native_calendar(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<(), AgentFailure> {
        let operation = native_inline_operation(target, "calendar.event_kit")?;
        super::calendar_access::apply_connection_observe(
            self.core,
            self.vault,
            self.calendar_subject,
            person_id,
            device_id,
            &operation,
            cancellation.clone(),
        )
        .await?;
        Ok(())
    }

    async fn enable_personal(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        connector: &str,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<(), AgentFailure> {
        if !matches!(
            connector,
            floe_access::ATTENTION_CONNECTOR | floe_access::WELLBEING_CONNECTOR
        ) {
            return Err(AgentFailure::InvalidInput);
        }
        let operation = native_inline_operation(target, connector)?;
        super::personal_access::apply_connection_observe(
            self.core,
            self.vault,
            self.personal_subject,
            person_id,
            device_id,
            &operation,
            cancellation.clone(),
        )
        .await?;
        Ok(())
    }

    /// Remote enable through the canonical ConnectionObserve path: re-read
    /// the reviewable bundle, compare every authority field with the
    /// reviewed target, then enable the fresh bundle. Provider identity
    /// and recipient are live routing facts the reviewed target never
    /// carried; they are verified live inside the enable, never adopted
    /// as reviewed authority.
    async fn enable_remote(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        connector: &str,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<(), AgentFailure> {
        let policies = crate::first_party_observe::remote_policies_for_target(
            self.vault,
            person_id,
            connector,
            &target.connection_id,
        )
        .await?;
        if policies.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let person_text = person_id.to_string();
        {
            let source_client =
                floe_provider_adapters::sources::ServerSourceClient::from_current_connection(
                    self.connections,
                    &person_text,
                    device_id,
                )?
                .ok_or(AgentFailure::PolicyDenied)?;
            let transport = floe_provider_adapters::sources::AuthorizedSourceClient::new(
                &source_client,
                self.vault,
            );
            let window = floe_access::RemoteCallWindow {
                deadline: self.probe_deadline,
                cancellation: cancellation.clone(),
            };
            let pairing = floe_access::RemotePairingIdentity {
                person_id: &person_text,
                device_id,
                client_id: source_client.source().client_id(),
            };
            let ctx = super::remote_observe::RemoteObserveContext {
                vault: self.vault,
                person_id,
                pairing,
                connector_id: connector,
                connection_id: target.connection_id.as_str(),
                window: &window,
            };
            enable_remote_reviewed(&ctx, &transport, target).await?;
            Ok(())
        }
    }
}

/// Enable a reviewed remote bundle through any grant transport: re-read
/// the reviewable bundle, compare every authority field with the reviewed
/// target, then enable the fresh bundle. Production passes the saved
/// connection transports; tests pass scripted previews. The transport
/// only attests live server truth; authority always comes from the
/// reviewed target plus the owner re-verification inside the enable.
pub(crate) async fn enable_remote_reviewed<Keys, Transport>(
    ctx: &super::remote_observe::RemoteObserveContext<'_, Keys>,
    transport: &Transport,
    reviewed: &floe_conversation::InlineObserveTarget,
) -> Result<(), AgentFailure>
where
    Keys: VaultKeyProvider,
    Transport: floe_access::RemoteGrantTransport,
{
    let fresh = super::remote_observe::review_bundle(ctx, transport).await?;
    compare_fresh_reviewed(&fresh, reviewed)?;
    super::remote_observe::enable_bundle(ctx, transport, &fresh).await
}

/// Compare a freshly re-read remote bundle with the reviewed target: every
/// authority field must equal the reviewed values. Provider identity and
/// recipient are live routing facts, verified live inside the enable; they
/// are never compared here because the reviewed target never bound them.
fn compare_fresh_reviewed(
    fresh: &crate::RemoteConnectionObserveExpectation,
    reviewed: &floe_conversation::InlineObserveTarget,
) -> Result<(), AgentFailure> {
    if fresh.members.len() != reviewed.members.len() {
        return Err(AgentFailure::AccessReviewRequired);
    }
    if let Some(reviewed_revision) = reviewed.connection_revision {
        if fresh
            .members
            .iter()
            .any(|member| member.connection_revision != Some(reviewed_revision))
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
    }
    let reviewed_producer = reviewed
        .reviewed_producer_fingerprint
        .as_deref()
        .ok_or(AgentFailure::InvalidInput)?;
    let reviewed_source = reviewed.source_revision.as_ref().and_then(|revision| {
        std::num::NonZeroU64::new(revision.epoch)
            .and_then(|epoch| SourceAuthority::from_parts(revision.incarnation, epoch))
    });
    if reviewed_source.is_some()
        && fresh
            .members
            .iter()
            .any(|member| Some(member.source_authority) != reviewed_source)
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    for member in &reviewed.members {
        let Some(current) = fresh
            .members
            .iter()
            .find(|candidate| candidate.view_id == member.member_id)
        else {
            return Err(AgentFailure::AccessReviewRequired);
        };
        if current.resource != member.resource
            || current.producer_fingerprint != reviewed_producer
            || current.policy_digest != member.policy_digest
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        match (
            &member.expected_grant,
            current.expected_grant_id,
            current.expected_grant_authority,
        ) {
            (floe_conversation::ExpectedGrantState::Absent, None, None) => {}
            (
                floe_conversation::ExpectedGrantState::Active {
                    grant_id,
                    authority_incarnation,
                    authority_epoch,
                },
                Some(id),
                Some(authority),
            ) if id.as_uuid() == *grant_id
                && authority.incarnation() == *authority_incarnation
                && authority.access_epoch().get() == *authority_epoch => {}
            _ => return Err(AgentFailure::AccessReviewRequired),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority() -> SourceAuthority {
        SourceAuthority::new()
    }

    fn reviewed_member() -> floe_conversation::ReviewedBundleMember {
        floe_conversation::ReviewedBundleMember {
            member_id: "mail.communication".into(),
            policy_digest: crate::first_party_observe::member_policy_digest(
                "gmail",
                "mail.communication",
            )
            .unwrap(),
            resource: "mail.communication:connection".into(),
            expected_grant: floe_conversation::ExpectedGrantState::Absent,
        }
    }

    fn reviewed_target() -> floe_conversation::InlineObserveTarget {
        floe_conversation::InlineObserveTarget {
            connection_id: "connection".into(),
            device_id: Some("device".into()),
            source_id: "floe.source.gmail".into(),
            connector_id: Some("gmail".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            source_revision: Some(floe_conversation::AuthorityRevision {
                incarnation: authority().incarnation(),
                epoch: 1,
            }),
            connection_revision: None,
            reviewed_producer_fingerprint: Some("producer".into()),
            reviewed_native_subject: None,
            members: vec![reviewed_member()],
        }
    }

    fn fresh_member(source: SourceAuthority) -> crate::RemoteObserveMemberExpectation {
        crate::RemoteObserveMemberExpectation {
            view_id: "mail.communication".into(),
            policy_digest: crate::first_party_observe::member_policy_digest(
                "gmail",
                "mail.communication",
            )
            .unwrap(),
            resource: "mail.communication:connection".into(),
            producer_fingerprint: "producer".into(),
            source_authority: source,
            connection_revision: Some(11),
            provider_identity: "google:subject-a".into(),
            recipient: "floe.server:instance".into(),
            expected_grant_id: None,
            expected_grant_authority: None,
        }
    }

    fn fresh_bundle(source: SourceAuthority) -> crate::RemoteConnectionObserveExpectation {
        crate::RemoteConnectionObserveExpectation {
            members: vec![fresh_member(source)],
        }
    }

    fn reviewed_source(target: &floe_conversation::InlineObserveTarget) -> SourceAuthority {
        let revision = target.source_revision.as_ref().unwrap();
        SourceAuthority::from_parts(
            revision.incarnation,
            std::num::NonZeroU64::new(revision.epoch).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn fresh_match_ignores_routing_but_binds_authority() {
        let target = reviewed_target();
        let source = reviewed_source(&target);
        // Exact authority match verifies even though provider identity and
        // recipient are live routing facts the review never carried.
        let mut fresh = fresh_bundle(source);
        fresh.members[0].provider_identity = "google:subject-b".into();
        fresh.members[0].recipient = "floe.server:other".into();
        assert_eq!(compare_fresh_reviewed(&fresh, &target), Ok(()));
        // Rotated source authority refuses.
        let rotated = fresh_bundle(SourceAuthority::new());
        assert_eq!(
            compare_fresh_reviewed(&rotated, &target),
            Err(AgentFailure::AccessReviewRequired)
        );
        // A live grant the review never described refuses.
        let mut granted = fresh_bundle(source);
        granted.members[0].expected_grant_id =
            Some(floe_access::GrantId::from_uuid(uuid::Uuid::new_v4()).unwrap());
        assert_eq!(
            compare_fresh_reviewed(&granted, &target),
            Err(AgentFailure::AccessReviewRequired)
        );
        // A changed member set refuses.
        let mut extra = fresh_bundle(source);
        extra.members.push(fresh_member(source));
        extra.members[1].view_id = "life.logistics".into();
        assert_eq!(
            compare_fresh_reviewed(&extra, &target),
            Err(AgentFailure::AccessReviewRequired)
        );
    }
}
