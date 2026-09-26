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
        let live = self
            .core
            .calendar_connection(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let Some(connection) = live else {
            return Ok(Self::unusable());
        };
        if connection.connection_id != target.connection_id
            || connection.device_id != device_id
            || connection.disconnected
            || connection.revision == 0
            || !connection.source_authority.is_valid()
        {
            return Ok(Self::unusable());
        }
        let selected: Vec<&str> = connection
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.as_str())
            .collect();
        if !target
            .members
            .iter()
            .all(|member| selected.contains(&member.resource.as_str()))
        {
            return Ok(Self::unusable());
        }
        let calendar_ids: Vec<String> = target
            .members
            .iter()
            .map(|member| member.resource.clone())
            .collect();
        let observation = floe_context::preview_native_calendar_subject(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
            self.calendar_subject,
            &floe_context::NativeCalendarSourceRequest {
                person_id,
                provider: connection.provider,
                device_id: device_id.to_owned(),
                calendar_ids,
                connection_scope: connection.scope,
                source_authority: Some(connection.source_authority),
                reviewed_native_subject_fingerprint: None,
                connection_id: Some(connection.connection_id.clone()),
            },
            &floe_access::RemoteCallWindow {
                deadline: self.probe_deadline,
                cancellation: cancellation.clone(),
            },
        )
        .await?;
        let reviewed_resources: Vec<String> = target
            .members
            .iter()
            .map(|member| member.resource.clone())
            .collect();
        let policy_fingerprint = crate::first_party_observe::policy_fingerprint(
            &crate::first_party_observe::native_calendar_policy_for_target(
                self.vault,
                person_id,
                connector,
                &target.connection_id,
                device_id,
                &reviewed_resources,
            )
            .await?,
        )?;
        let mut members = Vec::with_capacity(target.members.len());
        for member in &target.members {
            let mut live = self
                .probed_member(
                    connector,
                    &target.connection_id,
                    &member.member_id,
                    &member.resource,
                    Some(connection.source_authority),
                    person_id,
                    PolicyStore::Calendar,
                    Some(device_id),
                )
                .await?;
            live.policy_fingerprint = policy_fingerprint.clone();
            members.push(live);
        }
        Ok(LiveInlineState {
            members,
            connection_revision: Some(connection.revision),
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
        let (probe, identity_resource) = match connector {
            floe_access::ATTENTION_CONNECTOR => (
                floe_access::PersonalSubjectProbe::Attention,
                floe_access::ATTENTION_RESOURCE,
            ),
            floe_access::WELLBEING_CONNECTOR => (
                floe_access::PersonalSubjectProbe::Wellbeing,
                floe_access::WELLBEING_RESOURCE,
            ),
            _ => return Err(AgentFailure::CapabilityUnavailable),
        };
        if target.members.len() != 1
            || target.members[0].member_id != connector
            || target.members[0].resource != identity_resource
        {
            return Ok(Self::unusable());
        }
        let evidence = self
            .personal_subject
            .inspect(
                person_id,
                device_id,
                probe,
                None,
                Some(self.probe_deadline),
                cancellation.clone(),
            )
            .await?;
        if evidence.before != evidence.after {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let member = self
            .probed_member(
                connector,
                &target.connection_id,
                connector,
                identity_resource,
                None,
                person_id,
                PolicyStore::Personal,
                Some(device_id),
            )
            .await?;
        Ok(LiveInlineState {
            members: vec![member],
            connection_revision: None,
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
            target
                .members
                .first()
                .map(|member| member.resource.as_str()),
        )
        .await?;
        if policies.is_empty() {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let producer = match self.vault.remote_pinned_producer().await {
            Ok(producer) => producer,
            Err(_) => return Ok(Self::unusable()),
        };
        let calendar = policies.len() == 1 && policies[0].view_id == "calendar.timeline";
        if calendar {
            return self
                .read_remote_calendar(target, connector, person_id, producer.fingerprint)
                .await;
        }
        self.read_remote_views(target, connector, person_id, producer.fingerprint)
            .await
    }

    async fn read_remote_calendar(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        connector: &str,
        person_id: PersonId,
        producer_fingerprint: String,
    ) -> Result<LiveInlineState, AgentFailure> {
        if target.members.len() != 1 {
            return Ok(Self::unusable());
        }
        let member = &target.members[0];
        if member.member_id != "calendar.timeline" {
            return Ok(Self::unusable());
        }
        let live = self
            .core
            .calendar_connection(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let Some(connection) = live else {
            return Ok(Self::unusable());
        };
        if connection.connection_id != target.connection_id
            || connection.disconnected
            || connection.revision == 0
            || floe_access::hosted_calendar_connector(connection.provider).as_deref()
                != Some(connector)
            || !connection
                .calendars
                .iter()
                .any(|calendar| calendar.calendar_id == member.resource)
        {
            return Ok(Self::unusable());
        }
        let probed = self
            .probed_member(
                connector,
                &target.connection_id,
                &member.member_id,
                &member.resource,
                None,
                person_id,
                PolicyStore::Calendar,
                None,
            )
            .await?;
        Ok(LiveInlineState {
            members: vec![probed],
            connection_revision: Some(connection.revision),
            producer_fingerprint: Some(producer_fingerprint),
            native_subject: None,
            connection_usable: true,
        })
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
            None,
        )
        .await?;
        let canonical: Vec<(String, String)> = policies
            .iter()
            .map(|policy| {
                (
                    policy.view_id.to_owned(),
                    floe_context::remote_view_resource(policy.view_id, &target.connection_id),
                )
            })
            .collect();
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
                    None,
                    person_id,
                    PolicyStore::RemoteView,
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
                    None,
                    person_id,
                    PolicyStore::RemoteView,
                    None,
                )
                .await?,
            );
        }
        Ok(LiveInlineState {
            members,
            connection_revision: None,
            producer_fingerprint: Some(producer_fingerprint),
            native_subject: None,
            connection_usable: true,
        })
    }

    /// Probe one member key: live grants with their bound source authority,
    /// plus the recorded policy of a single live grant. A policy mapping
    /// that names a different grant refuses the read: the live state is
    /// ambiguous, not adoptable.
    #[allow(clippy::too_many_arguments)]
    async fn probed_member(
        &self,
        connector: &str,
        connection_id: &str,
        member_id: &str,
        resource: &str,
        source_revision: Option<SourceAuthority>,
        person_id: PersonId,
        policy: PolicyStore,
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
        let policy_authority = match grants.as_slice() {
            [grant] => Some(
                policy
                    .recorded(self.vault, connector, connection_id, member_id, grant)
                    .await?,
            ),
            _ => None,
        };
        Ok(LiveMember {
            member_id: member_id.to_owned(),
            policy_fingerprint: if connector == "calendar.event_kit" {
                crate::first_party_observe::policy_fingerprint(
                    &crate::first_party_observe::native_calendar_policy_for_target(
                        self.vault,
                        person_id,
                        connector,
                        connection_id,
                        device_id.ok_or(AgentFailure::InvalidInput)?,
                        &[resource.to_owned()],
                    )
                    .await?,
                )?
            } else if let Some(device_id) = device_id {
                crate::first_party_observe::native_member_policy_fingerprint_for_target(
                    self.vault, person_id, connector, device_id,
                )
                .await?
            } else if crate::first_party_observe::remote_policies(connector)?.is_empty() {
                crate::first_party_observe::member_policy_fingerprint(connector, member_id)?
            } else {
                crate::first_party_observe::remote_member_policy_fingerprint_for_target(
                    self.vault,
                    person_id,
                    connector,
                    connection_id,
                    member_id,
                    resource,
                )
                .await?
            },
            resource: resource.to_owned(),
            source_revision,
            live_grants: grants
                .into_iter()
                .map(|grant| LiveGrant {
                    id: grant.id(),
                    authority: grant.authority(),
                    source_authority: grant.source().source_authority(),
                })
                .collect(),
            policy_authority,
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
        if let Ok(Some(live)) = self.core.calendar_connection(person_id).await {
            if live.connection_id == connection_id && !live.disconnected {
                return Ok(true);
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
        if let Ok(Some(live)) = self.core.calendar_connection(person_id).await {
            if live.connection_id == connection_id
                && live.device_id == device_id
                && !live.disconnected
                && live.revision != 0
                && live.source_authority.is_valid()
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

    /// Native satisfaction: every selected calendar covered by exactly one
    /// live grant under a single connector, and the device subject
    /// readable. A probe failure is not satisfaction.
    async fn native_scope_satisfied(
        &self,
        connection: &floe_day::CalendarConnection,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<bool, AgentFailure> {
        if connection.calendars.is_empty() {
            return Ok(false);
        }
        let mut connector: Option<String> = None;
        for calendar in &connection.calendars {
            let covering = self
                .grants_for_connection_resource(
                    person_id,
                    &connection.connection_id,
                    &calendar.calendar_id,
                )
                .await?;
            let [grant] = covering.as_slice() else {
                return Ok(false);
            };
            let grant_connector = grant.source().connector().as_str().to_owned();
            match &connector {
                None => connector = Some(grant_connector),
                Some(known) if *known == grant_connector => {}
                Some(_) => return Ok(false),
            }
        }
        let calendar_ids: Vec<String> = connection
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.clone())
            .collect();
        let observation = floe_context::preview_native_calendar_subject(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
            self.calendar_subject,
            &floe_context::NativeCalendarSourceRequest {
                person_id,
                provider: connection.provider,
                device_id: device_id.to_owned(),
                calendar_ids,
                connection_scope: connection.scope,
                source_authority: Some(connection.source_authority),
                reviewed_native_subject_fingerprint: None,
                connection_id: Some(connection.connection_id.clone()),
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
        for (connector, probe) in [
            (
                floe_access::ATTENTION_CONNECTOR,
                floe_access::PersonalSubjectProbe::Attention,
            ),
            (
                floe_access::WELLBEING_CONNECTOR,
                floe_access::PersonalSubjectProbe::Wellbeing,
            ),
        ] {
            let grants = self
                .vault
                .list_data_access_grants(128)
                .await?
                .into_iter()
                .filter(|grant| {
                    grant.source().person_id() == person_id
                        && grant.source().connector().as_str() == connector
                        && grant.source().connection_id().as_str() == connection_id
                        && grant.state() != floe_access::GrantState::Revoked
                })
                .collect::<Vec<_>>();
            if grants.len() != 1 {
                continue;
            }
            let evidence = self
                .personal_subject
                .inspect(
                    person_id,
                    device_id,
                    probe,
                    None,
                    Some(self.probe_deadline),
                    cancellation.clone(),
                )
                .await;
            match evidence {
                Ok(evidence) if evidence.before == evidence.after => return Ok(true),
                _ => continue,
            }
        }
        Ok(false)
    }

    /// Remote satisfaction: some remote connector on this connection has
    /// every canonical view covered by exactly one live grant, and the
    /// remote authority is still paired. A satisfied-then-revoked scope
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
                let resource = floe_context::remote_view_resource(policy.view_id, connection_id);
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

    /// Live non-revoked grants for one connection resource across every
    /// connector. Callers attribute the connector from the grants'
    /// own source bindings.
    async fn grants_for_connection_resource(
        &self,
        person_id: PersonId,
        connection_id: &str,
        resource: &str,
    ) -> Result<Vec<floe_access::DataAccessGrant>, AgentFailure> {
        Ok(self
            .vault
            .list_data_access_grants(128)
            .await?
            .into_iter()
            .filter(|grant| {
                grant.source().person_id() == person_id
                    && grant.source().connection_id().as_str() == connection_id
                    && grant.state() != floe_access::GrantState::Revoked
                    && grant
                        .scope()
                        .resources()
                        .iter()
                        .any(|value| value.as_str() == resource)
            })
            .collect())
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

    /// Native enable through the canonical Calendar Review: the reviewed
    /// selection, source authority, native subject and grant expectation
    /// travel verbatim into the same operation the connection screen uses.
    async fn enable_native_calendar(
        &self,
        target: &floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<(), AgentFailure> {
        let first = target.members.first().ok_or(AgentFailure::InvalidInput)?;
        for member in &target.members {
            if member.source_revision != first.source_revision
                || member.expected_grant != first.expected_grant
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        let source_authority = first
            .source_revision
            .as_ref()
            .and_then(|revision| {
                std::num::NonZeroU64::new(revision.epoch)
                    .and_then(|epoch| SourceAuthority::from_parts(revision.incarnation, epoch))
            })
            .ok_or(AgentFailure::InvalidInput)?;
        let (expected_grant_id, expected_grant_authority) = match &first.expected_grant {
            floe_conversation::ExpectedGrantState::Absent => (None, None),
            floe_conversation::ExpectedGrantState::Active {
                grant_id,
                authority_incarnation,
                authority_epoch,
            } => {
                let id =
                    floe_access::GrantId::from_uuid(*grant_id).ok_or(AgentFailure::InvalidInput)?;
                let authority = std::num::NonZeroU64::new(*authority_epoch)
                    .and_then(|epoch| {
                        floe_access::GrantAuthority::from_parts(*authority_incarnation, epoch)
                    })
                    .ok_or(AgentFailure::InvalidInput)?;
                (Some(id), Some(authority))
            }
        };
        let fingerprint = target
            .reviewed_native_subject
            .clone()
            .ok_or(AgentFailure::InvalidInput)?;
        let calendar_ids: Vec<String> = target
            .members
            .iter()
            .map(|member| member.resource.clone())
            .collect();
        super::calendar_access::apply_calendar_access(
            self.core,
            self.vault,
            self.calendar_subject,
            person_id,
            device_id.to_owned(),
            crate::CalendarAccessChange::Review {
                connection_id: target.connection_id.clone(),
                calendar_ids,
                expected_source_authority: source_authority,
                expected_native_subject_fingerprint: fingerprint,
                expected_grant_id,
                expected_grant_authority,
            },
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
        let identity_resource = match connector {
            floe_access::ATTENTION_CONNECTOR => floe_access::ATTENTION_RESOURCE,
            floe_access::WELLBEING_CONNECTOR => floe_access::WELLBEING_RESOURCE,
            _ => return Err(AgentFailure::InvalidInput),
        };
        if target.members.len() != 1
            || target.members[0].member_id != connector
            || target.members[0].resource != identity_resource
        {
            return Err(AgentFailure::InvalidInput);
        }
        let fingerprint = target
            .reviewed_native_subject
            .clone()
            .ok_or(AgentFailure::InvalidInput)?;
        let (expected_grant_id, expected_grant_authority) = match &target.members[0].expected_grant
        {
            floe_conversation::ExpectedGrantState::Absent => (None, None),
            floe_conversation::ExpectedGrantState::Active {
                grant_id,
                authority_incarnation,
                authority_epoch,
            } => {
                let id =
                    floe_access::GrantId::from_uuid(*grant_id).ok_or(AgentFailure::InvalidInput)?;
                let authority = std::num::NonZeroU64::new(*authority_epoch)
                    .and_then(|epoch| {
                        floe_access::GrantAuthority::from_parts(*authority_incarnation, epoch)
                    })
                    .ok_or(AgentFailure::InvalidInput)?;
                (Some(id), Some(authority))
            }
        };
        let consumers = crate::first_party_observe::native_consumers_for_target(
            self.vault, person_id, connector, device_id,
        )
        .await?;
        floe_access::apply_personal_access(
            self.vault,
            self.personal_subject,
            person_id,
            floe_access::PersonalAccessConfiguration {
                connector: connector.to_owned(),
                device_id: device_id.to_owned(),
                consumers,
                change: floe_access::PersonalAccessChange::Review {
                    expected_native_subject_fingerprint: fingerprint,
                    feasibility_query: None,
                    expected_grant_id,
                    expected_grant_authority,
                },
            },
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
            target
                .members
                .first()
                .map(|member| member.resource.as_str()),
        )
        .await?;
        if policies.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let person_text = person_id.to_string();
        let calendar = policies.len() == 1 && policies[0].view_id == "calendar.timeline";
        if calendar {
            if target.members.len() != 1 {
                return Err(AgentFailure::InvalidInput);
            }
            let resource = target.members[0].resource.as_str();
            let transport =
                floe_provider_adapters::control::authorization::RemoteAuthorityEndpoint::from_current_connection(
                    self.connections,
                    &person_text,
                    device_id,
                    Some(self.vault),
                )?;
            let window = super::remote_authority::authority_window(cancellation.clone());
            let pairing = floe_access::RemotePairingIdentity {
                person_id: &person_text,
                device_id,
                client_id: transport.client_id(),
            };
            let ctx = super::remote_observe::RemoteObserveContext {
                core: self.core,
                vault: self.vault,
                person_id,
                pairing,
                connector_id: connector,
                connection_id: target.connection_id.as_str(),
                resource: Some(resource),
                window: &window,
            };
            enable_remote_reviewed(&ctx, &transport, target).await?;
            Ok(())
        } else {
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
                core: self.core,
                vault: self.vault,
                person_id,
                pairing,
                connector_id: connector,
                connection_id: target.connection_id.as_str(),
                resource: None,
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

/// Which policy store recorded one member's reviewed grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PolicyStore {
    Calendar,
    Personal,
    RemoteView,
}

impl PolicyStore {
    async fn recorded<Keys: VaultKeyProvider>(
        self,
        vault: &EncryptedAgentVault<Keys>,
        connector: &str,
        connection_id: &str,
        member_id: &str,
        grant: &floe_access::DataAccessGrant,
    ) -> Result<floe_context_contract::ConsumerPolicyAuthority, AgentFailure> {
        match self {
            Self::Calendar => vault.calendar_grant_policy_authority(grant.id()).await,
            Self::Personal => vault.personal_grant_consumer_policy(grant.id()).await,
            Self::RemoteView => {
                let (recorded, policy) = vault
                    .remote_view_grant_policy(
                        member_id,
                        connector,
                        connection_id,
                        grant.source().source_authority(),
                    )
                    .await?;
                if recorded != grant.id() {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                Ok(policy)
            }
        }
    }
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
            || current.policy_fingerprint != member.policy_fingerprint
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let reviewed_source = member.source_revision.as_ref().and_then(|revision| {
            std::num::NonZeroU64::new(revision.epoch)
                .and_then(|epoch| SourceAuthority::from_parts(revision.incarnation, epoch))
        });
        if reviewed_source.is_some() && Some(current.source_authority) != reviewed_source {
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
        let reviewed_policy = member.policy_authority.as_ref().and_then(|revision| {
            std::num::NonZeroU64::new(revision.epoch).and_then(|epoch| {
                floe_context_contract::ConsumerPolicyAuthority::from_parts(
                    revision.incarnation,
                    epoch,
                )
            })
        });
        if reviewed_policy.is_some() && current.expected_policy != reviewed_policy {
            return Err(AgentFailure::AccessReviewRequired);
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
        let source = authority();
        floe_conversation::ReviewedBundleMember {
            member_id: "mail.communication".into(),
            policy_fingerprint: crate::first_party_observe::member_policy_fingerprint(
                "gmail",
                "mail.communication",
            )
            .unwrap(),
            resource: "mail.communication:connection".into(),
            source_revision: Some(floe_conversation::AuthorityRevision {
                incarnation: source.incarnation(),
                epoch: source.epoch().get(),
            }),
            expected_grant: floe_conversation::ExpectedGrantState::Absent,
            policy_authority: None,
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
            connection_revision: None,
            reviewed_producer_fingerprint: Some("producer".into()),
            reviewed_native_subject: None,
            members: vec![reviewed_member()],
        }
    }

    fn fresh_member(source: SourceAuthority) -> crate::RemoteObserveMemberExpectation {
        crate::RemoteObserveMemberExpectation {
            view_id: "mail.communication".into(),
            policy_fingerprint: crate::first_party_observe::member_policy_fingerprint(
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
            expected_policy: None,
        }
    }

    fn fresh_bundle(source: SourceAuthority) -> crate::RemoteConnectionObserveExpectation {
        crate::RemoteConnectionObserveExpectation {
            members: vec![fresh_member(source)],
        }
    }

    fn reviewed_source(member: &floe_conversation::ReviewedBundleMember) -> SourceAuthority {
        let revision = member.source_revision.as_ref().unwrap();
        SourceAuthority::from_parts(
            revision.incarnation,
            std::num::NonZeroU64::new(revision.epoch).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn fresh_match_ignores_routing_but_binds_authority() {
        let target = reviewed_target();
        let source = reviewed_source(&target.members[0]);
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
