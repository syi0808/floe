//! Capture of the reviewed owner identity at interaction publication.
//!
//! An inline Observe card binds more than the blocked read named: the whole
//! affected bundle with per-member grant expectations, plus the owner
//! identity the mutation will be judged against (local connection revision,
//! pinned remote producer, live native subject). This module captures that
//! snapshot from the owners while the owner-produced requirement is fresh.
//!
//! Two rules keep the capture honest:
//!
//! - The blocked member's source revision and grant expectation come from the
//!   requirement verbatim. They were observed at classification, which is the
//!   review time; re-reading latest and substituting it would swap the
//!   expected authority the decision binds.
//! - Everything else is read now, because capture is its review time: sibling
//!   bundle members, the live native subject, the pinned producer, the local
//!   connection revision and the recorded policy of the reviewed grant. A
//!   sibling live grant the requirement never named is reviewed as observed,
//!   never silently adopted later.
//!
//! Any capture failure downgrades the card to navigation-only: an unresolved
//! target with insufficient current identity gets navigation/review actions,
//! never an inline mutation it cannot bind.

use floe_access::{GrantAuthority, GrantId};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_context_contract::{ConsumerPolicyAuthority, SourceAccessRequirement, SourceAuthority};
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

/// One reviewed grant of the captured bundle.
#[derive(Clone, Debug)]
pub(crate) struct SnapshotMember {
    pub member_id: String,
    pub resource: String,
    pub source_revision: Option<SourceAuthority>,
    /// The reviewed grant expectation: `None` is reviewed absence.
    pub expected_grant: Option<(GrantId, GrantAuthority)>,
    pub policy_authority: Option<ConsumerPolicyAuthority>,
}

/// The owner identity an inline Observe decision binds.
#[derive(Clone, Debug)]
pub(crate) struct InlineReviewSnapshot {
    pub members: Vec<SnapshotMember>,
    pub connection_revision: Option<u64>,
    pub producer_fingerprint: Option<String>,
    pub native_subject: Option<String>,
}

/// Captures the reviewed snapshot for an inline-eligible requirement.
///
/// Implementations read owner truth (never model output): connections, grants
/// and policies from the vault, the live native subject from the device, the
/// pinned producer from remote authority. Failure means the card navigates.
pub(crate) trait ReviewSnapshotSource: Send + Sync {
    fn capture_inline<'a>(
        &'a self,
        requirement: &'a SourceAccessRequirement,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<InlineReviewSnapshot, AgentFailure>>;
}

/// Capture that always declines: every inline-eligible requirement navigates.
#[cfg(test)]
pub(crate) struct NoCaptureSnapshots;

#[cfg(test)]
impl ReviewSnapshotSource for NoCaptureSnapshots {
    fn capture_inline<'a>(
        &'a self,
        _requirement: &'a SourceAccessRequirement,
        _person_id: PersonId,
        _device_id: &'a str,
        _cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<InlineReviewSnapshot, AgentFailure>> {
        Box::pin(async move { Err(AgentFailure::CapabilityUnavailable) })
    }
}

/// The host's capture: core connections, vault grants and device subjects.
pub(crate) struct HostReviewSnapshots<'a, Keys: VaultKeyProvider, CalendarSubject> {
    pub core: &'a crate::FloeCore,
    pub vault: &'a EncryptedAgentVault<Keys>,
    pub calendar_subject: &'a CalendarSubject,
    pub personal_subject: &'a dyn floe_access::PersonalSubjectInspector,
    /// Capture must stay fast: probes run under this deadline, not the turn's.
    pub capture_deadline: tokio::time::Instant,
}

impl<Keys, CalendarSubject> ReviewSnapshotSource for HostReviewSnapshots<'_, Keys, CalendarSubject>
where
    Keys: VaultKeyProvider,
    CalendarSubject: floe_context::NativeCalendarSubjectSource,
{
    fn capture_inline<'a>(
        &'a self,
        requirement: &'a SourceAccessRequirement,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<InlineReviewSnapshot, AgentFailure>> {
        Box::pin(async move {
            self.capture(requirement, person_id, device_id, cancellation)
                .await
        })
    }
}

impl<Keys, CalendarSubject> HostReviewSnapshots<'_, Keys, CalendarSubject>
where
    Keys: VaultKeyProvider,
    CalendarSubject: floe_context::NativeCalendarSubjectSource,
{
    async fn capture(
        &self,
        requirement: &SourceAccessRequirement,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<InlineReviewSnapshot, AgentFailure> {
        requirement
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if self.vault.person_id() != person_id
            || device_id.trim().is_empty()
            || requirement.resources().is_empty()
        {
            return Err(AgentFailure::InvalidInput);
        }
        let connector = requirement
            .connector_id()
            .map(|id| id.as_str())
            .ok_or(AgentFailure::InvalidInput)?;
        let connection = requirement
            .connection_id()
            .map(|id| id.as_str())
            .ok_or(AgentFailure::InvalidInput)?;
        if floe_access::native_calendar_provider(connector).is_some() {
            return self
                .capture_native_calendar(requirement, person_id, device_id, cancellation)
                .await;
        }
        if matches!(
            connector,
            floe_access::ATTENTION_CONNECTOR | floe_access::WELLBEING_CONNECTOR
        ) {
            return self
                .capture_personal(requirement, person_id, device_id, cancellation)
                .await;
        }
        if !crate::first_party_observe::remote_policies(connector)?.is_empty() {
            return self
                .capture_remote(requirement, person_id, connector, connection)
                .await;
        }
        // Unknown connectors, query-bound feasibility and picker-owned
        // contacts selection cannot bind an inline mutation: navigate.
        Err(AgentFailure::CapabilityUnavailable)
    }

    /// Native calendar: members are the reviewed calendars, the subject is
    /// probed live, and the local connection revision binds the review.
    async fn capture_native_calendar(
        &self,
        requirement: &SourceAccessRequirement,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<InlineReviewSnapshot, AgentFailure> {
        let connection_id = requirement
            .connection_id()
            .map(|id| id.as_str())
            .ok_or(AgentFailure::InvalidInput)?;
        let live = self
            .core
            .calendar_connection(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        if live.connection_id != connection_id
            || live.device_id != device_id
            || live.disconnected
            || live.revision == 0
            || !live.source_authority.is_valid()
            || requirement.source_authority() != Some(live.source_authority)
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let selected: Vec<String> = live
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.clone())
            .collect();
        let reviewed: Vec<String> = requirement
            .resources()
            .iter()
            .map(|resource| resource.as_str().to_owned())
            .collect();
        if !reviewed.iter().all(|id| selected.contains(id)) {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let observation = floe_context::preview_native_calendar_subject(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
            self.calendar_subject,
            &floe_context::NativeCalendarSourceRequest {
                person_id,
                provider: live.provider,
                device_id: device_id.to_owned(),
                calendar_ids: reviewed.clone(),
                connection_scope: live.scope,
                source_authority: requirement.source_authority(),
                reviewed_native_subject_fingerprint: None,
                connection_id: Some(live.connection_id.clone()),
            },
            &floe_access::RemoteCallWindow {
                deadline: self.capture_deadline,
                cancellation: cancellation.clone(),
            },
        )
        .await?;
        let expected = observed_expectation(requirement);
        let connector = requirement
            .connector_id()
            .map(|id| id.as_str())
            .ok_or(AgentFailure::InvalidInput)?;
        self.verify_observed_live(person_id, connector, connection_id, expected)
            .await?;
        let mut members = Vec::with_capacity(reviewed.len());
        for resource in reviewed {
            members.push(SnapshotMember {
                member_id: "calendar.timeline".to_owned(),
                resource,
                source_revision: requirement.source_authority(),
                expected_grant: expected,
                policy_authority: self.calendar_policy(expected).await?,
            });
        }
        members.sort_by(|left, right| {
            left.member_id
                .cmp(&right.member_id)
                .then_with(|| left.resource.cmp(&right.resource))
        });
        Ok(InlineReviewSnapshot {
            members,
            connection_revision: Some(live.revision),
            producer_fingerprint: None,
            native_subject: Some(observation.native_subject_fingerprint),
        })
    }

    /// Personal attention/wellbeing: one reviewed member, one live subject.
    async fn capture_personal(
        &self,
        requirement: &SourceAccessRequirement,
        person_id: PersonId,
        device_id: &str,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<InlineReviewSnapshot, AgentFailure> {
        let connector = requirement
            .connector_id()
            .map(|id| id.as_str())
            .ok_or(AgentFailure::InvalidInput)?;
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
        if requirement.resources().len() != 1
            || requirement.resources()[0].as_str() != identity_resource
        {
            return Err(AgentFailure::InvalidInput);
        }
        let evidence = self
            .personal_subject
            .inspect(
                person_id,
                device_id,
                probe,
                None,
                Some(self.capture_deadline),
                cancellation.clone(),
            )
            .await?;
        if evidence.before != evidence.after {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let expected = observed_expectation(requirement);
        let connection = requirement
            .connection_id()
            .map(|id| id.as_str())
            .ok_or(AgentFailure::InvalidInput)?;
        self.verify_observed_live(person_id, connector, connection, expected)
            .await?;
        Ok(InlineReviewSnapshot {
            members: vec![SnapshotMember {
                member_id: connector.to_owned(),
                resource: identity_resource.to_owned(),
                source_revision: requirement.source_authority(),
                expected_grant: expected,
                policy_authority: self.personal_policy(expected).await?,
            }],
            connection_revision: None,
            producer_fingerprint: None,
            native_subject: Some(evidence.before),
        })
    }

    /// Remote views and remote calendar: the full canonical bundle with the
    /// blocked member verbatim from the requirement and siblings as observed
    /// now, bound to the pinned producer.
    async fn capture_remote(
        &self,
        requirement: &SourceAccessRequirement,
        person_id: PersonId,
        connector: &str,
        connection: &str,
    ) -> Result<InlineReviewSnapshot, AgentFailure> {
        let policies = crate::first_party_observe::remote_policies(connector)?;
        if policies.is_empty() {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let producer = self.vault.remote_pinned_producer().await?;
        let requested: Vec<String> = requirement
            .resources()
            .iter()
            .map(|resource| resource.as_str().to_owned())
            .collect();
        let mut members = Vec::with_capacity(policies.len());
        let mut connection_revision = None;
        for policy in &policies {
            let resource = if policy.view_id == "calendar.timeline" {
                if connector == "calendar.google" || connector == "calendar.microsoft" {
                    if requested.len() != 1 {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    connection_revision = Some(
                        self.verified_remote_calendar_resource(
                            person_id,
                            connector,
                            connection,
                            requirement,
                            &requested[0],
                        )
                        .await?,
                    );
                    requested[0].clone()
                } else {
                    return Err(AgentFailure::InvalidInput);
                }
            } else {
                floe_context::remote_view_resource(policy.view_id, connection)
            };
            let blocked = requested.contains(&resource);
            let expected = if blocked {
                let expected = observed_expectation(requirement);
                self.verify_blocked_grant(person_id, connector, connection, &resource, expected)
                    .await?;
                expected
            } else {
                self.sibling_grant(person_id, connector, connection, &resource)
                    .await?
            };
            let policy_authority = if policy.view_id == "calendar.timeline" {
                self.calendar_policy(expected).await?
            } else {
                self.remote_view_policy(
                    policy.view_id,
                    connector,
                    connection,
                    requirement,
                    expected,
                )
                .await?
            };
            members.push(SnapshotMember {
                member_id: policy.view_id.to_owned(),
                resource,
                source_revision: requirement.source_authority(),
                expected_grant: expected,
                policy_authority,
            });
        }
        for resource in &requested {
            if !members.iter().any(|member| &member.resource == resource) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
        }
        members.sort_by(|left, right| {
            left.member_id
                .cmp(&right.member_id)
                .then_with(|| left.resource.cmp(&right.resource))
        });
        Ok(InlineReviewSnapshot {
            members,
            connection_revision,
            producer_fingerprint: Some(producer.fingerprint),
            native_subject: None,
        })
    }

    /// The live non-revoked grants naming one bundle member resource.
    async fn live_member_grants(
        &self,
        person_id: PersonId,
        connector: &str,
        connection: &str,
        resource: &str,
    ) -> Result<Vec<floe_access::DataAccessGrant>, AgentFailure> {
        Ok(self
            .vault
            .list_data_access_grants(128)
            .await?
            .into_iter()
            .filter(|grant| {
                grant.source().person_id() == person_id
                    && grant.source().connector().as_str() == connector
                    && grant.source().connection_id().as_str() == connection
                    && grant.state() != floe_access::GrantState::Revoked
                    && grant
                        .scope()
                        .resources()
                        .iter()
                        .any(|value| value.as_str() == resource)
            })
            .collect())
    }

    /// The blocked member's reviewed expectation still holds now: the
    /// observed grant is unchanged, or absence is still proven. Anything
    /// else navigates to a fresh review instead of adopting the new state.
    async fn verify_blocked_grant(
        &self,
        person_id: PersonId,
        connector: &str,
        connection: &str,
        resource: &str,
        expected: Option<(GrantId, GrantAuthority)>,
    ) -> Result<(), AgentFailure> {
        let live = self
            .live_member_grants(person_id, connector, connection, resource)
            .await?;
        match expected {
            Some((grant_id, authority)) => match live.as_slice() {
                [grant] if grant.id() == grant_id && grant.authority() == authority => Ok(()),
                _ => Err(AgentFailure::AccessReviewRequired),
            },
            None => {
                if live.is_empty() {
                    Ok(())
                } else {
                    Err(AgentFailure::AccessReviewRequired)
                }
            }
        }
    }

    /// The observed grant of a native/personal review still holds now.
    async fn verify_observed_live(
        &self,
        person_id: PersonId,
        connector: &str,
        connection: &str,
        expected: Option<(GrantId, GrantAuthority)>,
    ) -> Result<(), AgentFailure> {
        let Some((grant_id, authority)) = expected else {
            return Ok(());
        };
        let live = self.vault.get_data_access_grant(grant_id).await?;
        if live.source().person_id() != person_id
            || live.source().connector().as_str() != connector
            || live.source().connection_id().as_str() != connection
            || live.authority() != authority
            || live.state() == floe_access::GrantState::Revoked
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        Ok(())
    }

    /// A sibling bundle member as observed now: one live grant is reviewed
    /// as observed, proven absence as absence. Duplicates navigate.
    async fn sibling_grant(
        &self,
        person_id: PersonId,
        connector: &str,
        connection: &str,
        resource: &str,
    ) -> Result<Option<(GrantId, GrantAuthority)>, AgentFailure> {
        let live = self
            .live_member_grants(person_id, connector, connection, resource)
            .await?;
        match live.as_slice() {
            [] => Ok(None),
            [grant] => Ok(Some((grant.id(), grant.authority()))),
            _ => Err(AgentFailure::Conflict),
        }
    }

    /// The reviewed remote calendar still names the reviewed connection and
    /// calendar. The authoritative descriptor revision is server-owned, so
    /// the review also binds the local connection revision: a scope update
    /// between capture and decision fails the enable instead of widening it.
    async fn verified_remote_calendar_resource(
        &self,
        person_id: PersonId,
        connector: &str,
        connection: &str,
        requirement: &SourceAccessRequirement,
        resource: &str,
    ) -> Result<u64, AgentFailure> {
        let live = self
            .core
            .calendar_connection(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let hosted = floe_access::hosted_calendar_connector(live.provider);
        if live.connection_id != connection
            || live.disconnected
            || live.revision == 0
            || hosted != Some(connector)
            || requirement.source_authority() != Some(live.source_authority)
            || !live
                .calendars
                .iter()
                .any(|calendar| calendar.calendar_id == resource)
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        Ok(live.revision)
    }

    async fn calendar_policy(
        &self,
        expected: Option<(GrantId, GrantAuthority)>,
    ) -> Result<Option<ConsumerPolicyAuthority>, AgentFailure> {
        match expected {
            None => Ok(None),
            Some((grant_id, _)) => self
                .vault
                .calendar_grant_policy_authority(grant_id)
                .await
                .map(Some),
        }
    }

    async fn personal_policy(
        &self,
        expected: Option<(GrantId, GrantAuthority)>,
    ) -> Result<Option<ConsumerPolicyAuthority>, AgentFailure> {
        match expected {
            None => Ok(None),
            Some((grant_id, _)) => self
                .vault
                .personal_grant_consumer_policy(grant_id)
                .await
                .map(Some),
        }
    }

    /// The recorded policy of a reviewed remote-view grant: the mapping must
    /// name the reviewed grant id, never a substituted one.
    #[allow(clippy::too_many_arguments)]
    async fn remote_view_policy(
        &self,
        view_id: &str,
        connector: &str,
        connection: &str,
        requirement: &SourceAccessRequirement,
        expected: Option<(GrantId, GrantAuthority)>,
    ) -> Result<Option<ConsumerPolicyAuthority>, AgentFailure> {
        let Some((grant_id, _)) = expected else {
            return Ok(None);
        };
        let authority = requirement
            .source_authority()
            .ok_or(AgentFailure::InvalidInput)?;
        let (recorded, policy) = self
            .vault
            .remote_view_grant_policy(view_id, connector, connection, authority)
            .await?;
        if recorded != grant_id {
            return Err(AgentFailure::AccessReviewRequired);
        }
        Ok(Some(policy))
    }
}

fn observed_expectation(
    requirement: &SourceAccessRequirement,
) -> Option<(GrantId, GrantAuthority)> {
    requirement
        .observed_grant()
        .map(|observed| (observed.grant_id(), observed.authority()))
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A scripted calendar subject: answers one fingerprint for any probe.
    pub(crate) struct FixtureCalendarSubject {
        pub fingerprint: String,
    }

    impl floe_context::NativeCalendarSubjectSource for FixtureCalendarSubject {
        async fn subject(
            &self,
            _request: floe_context::NativeSubjectRequest,
        ) -> Result<floe_context::NativeSubjectObservation, AgentFailure> {
            Ok(floe_context::NativeSubjectObservation {
                before: self.fingerprint.clone(),
                after: None,
            })
        }
    }

    /// A scripted personal subject inspector: stable evidence, no presence.
    pub(crate) struct FixturePersonalInspector {
        pub fingerprint: String,
    }

    impl floe_access::PersonalSubjectInspector for FixturePersonalInspector {
        fn inspect<'a>(
            &'a self,
            _person_id: PersonId,
            _device_id: &'a str,
            _probe: floe_access::PersonalSubjectProbe<'a>,
            _expected_native_subject_fingerprint: Option<String>,
            _deadline: Option<tokio::time::Instant>,
            _cancellation: floe_execution::Cancellation,
        ) -> BoxFuture<'a, Result<floe_access::PersonalSubjectEvidence, AgentFailure>> {
            let fingerprint = self.fingerprint.clone();
            Box::pin(async move {
                Ok(floe_access::PersonalSubjectEvidence {
                    before: fingerprint.clone(),
                    after: fingerprint,
                })
            })
        }

        fn attention_presence(&self, _person_id: PersonId, _device_id: &str) -> Option<uuid::Uuid> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use floe_context_contract::{
        ConnectionId, ConnectorId, GrantConsumer, GrantOperation, GrantPurpose, ResourceHandle,
        SourceAccessRequirementKind,
    };

    use super::fixtures::{FixtureCalendarSubject, FixturePersonalInspector};
    use super::*;

    #[derive(Default)]
    struct Keys {
        key: std::sync::Mutex<std::collections::HashMap<(PersonId, uuid::Uuid), [u8; 32]>>,
    }

    impl floe_vault::VaultKeyProvider for Keys {
        fn load(
            &self,
            person_id: PersonId,
            vault_id: uuid::Uuid,
        ) -> Result<floe_vault::VaultKey, AgentFailure> {
            self.key
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(floe_vault::VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: uuid::Uuid,
            key: &floe_vault::VaultKey,
        ) -> Result<(), AgentFailure> {
            self.key
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    struct Fixture {
        core: crate::FloeCore,
        vault: EncryptedAgentVault<Keys>,
        person_id: PersonId,
        _root: tempfile::TempDir,
    }

    impl Fixture {
        async fn open() -> Self {
            let core = crate::FloeCore::open(":memory:").await.unwrap();
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let person_id = PersonId::new();
            let vault = EncryptedAgentVault::create(root.path(), person_id, Keys::default())
                .await
                .unwrap();
            Self {
                core,
                vault,
                person_id,
                _root: root,
            }
        }

        fn snapshots<'a>(
            &'a self,
            calendar: &'a FixtureCalendarSubject,
            personal: &'a FixturePersonalInspector,
        ) -> HostReviewSnapshots<'a, Keys, FixtureCalendarSubject> {
            HostReviewSnapshots {
                core: &self.core,
                vault: &self.vault,
                calendar_subject: calendar,
                personal_subject: personal,
                capture_deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            }
        }
    }

    fn requirement(
        source_id: &str,
        connector: Option<&str>,
        connection: Option<&str>,
        resources: Vec<&str>,
        reason: SourceAccessRequirementKind,
    ) -> SourceAccessRequirement {
        SourceAccessRequirement::try_new(
            source_id,
            connector.map(|id| ConnectorId::try_new(id).unwrap()),
            connection.map(|id| ConnectionId::try_new(id).unwrap()),
            GrantOperation::Read,
            GrantConsumer::builtin("assistant").unwrap(),
            GrantPurpose::Assistant,
            resources
                .into_iter()
                .map(|resource| ResourceHandle::try_new(resource).unwrap())
                .collect(),
            None,
            reason,
            None,
            None,
            true,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn unknown_and_picker_owned_connectors_decline_capture() {
        let fixture = Fixture::open().await;
        let calendar = FixtureCalendarSubject {
            fingerprint: "a".repeat(64),
        };
        let personal = FixturePersonalInspector {
            fingerprint: "b".repeat(64),
        };
        let snapshots = fixture.snapshots(&calendar, &personal);
        let cancellation = floe_execution::Cancellation::default();
        for connector in [
            "unknown.connector",
            "feasibility.apple",
            "contacts.apple",
            "contacts.android",
        ] {
            let error = snapshots
                .capture_inline(
                    &requirement(
                        "floe.source.contacts",
                        Some(connector),
                        Some("connection"),
                        vec!["people.identity"],
                        SourceAccessRequirementKind::EnableObserve,
                    ),
                    fixture.person_id,
                    "device",
                    &cancellation,
                )
                .await
                .err()
                .unwrap();
            assert_eq!(error, AgentFailure::CapabilityUnavailable, "{connector}");
        }
    }

    #[tokio::test]
    async fn attention_capture_binds_live_subject_and_reviewed_absence() {
        let fixture = Fixture::open().await;
        let calendar = FixtureCalendarSubject {
            fingerprint: "a".repeat(64),
        };
        let personal = FixturePersonalInspector {
            fingerprint: "c".repeat(64),
        };
        let snapshots = fixture.snapshots(&calendar, &personal);
        let cancellation = floe_execution::Cancellation::default();
        let snapshot = snapshots
            .capture_inline(
                &requirement(
                    "floe.source.attention",
                    Some(floe_access::ATTENTION_CONNECTOR),
                    Some(floe_access::ATTENTION_CONNECTION),
                    vec![floe_access::ATTENTION_RESOURCE],
                    SourceAccessRequirementKind::EnableObserve,
                ),
                fixture.person_id,
                "device",
                &cancellation,
            )
            .await
            .unwrap();
        assert_eq!(snapshot.members.len(), 1);
        assert_eq!(
            snapshot.members[0].member_id,
            floe_access::ATTENTION_CONNECTOR
        );
        assert_eq!(
            snapshot.members[0].resource,
            floe_access::ATTENTION_RESOURCE
        );
        assert_eq!(snapshot.members[0].expected_grant, None);
        assert_eq!(snapshot.members[0].policy_authority, None);
        assert_eq!(
            snapshot.native_subject.as_deref(),
            Some("c".repeat(64).as_str())
        );
        assert_eq!(snapshot.connection_revision, None);
        assert_eq!(snapshot.producer_fingerprint, None);
    }

    #[tokio::test]
    async fn native_calendar_capture_binds_revision_subject_and_selection() {
        let fixture = Fixture::open().await;
        fixture
            .core
            .set_calendar_scope(
                fixture.person_id,
                "fixture-connection".into(),
                3,
                "device".into(),
                floe_context_contract::CalendarProvider::EventKit,
                vec![crate::CalendarSelection {
                    calendar_id: "home".into(),
                    calendar_name: "Home".into(),
                }],
                floe_context_contract::CalendarScope::Selected,
            )
            .await
            .unwrap();
        let calendar = FixtureCalendarSubject {
            fingerprint: "d".repeat(64),
        };
        let personal = FixturePersonalInspector {
            fingerprint: "c".repeat(64),
        };
        let snapshots = fixture.snapshots(&calendar, &personal);
        let cancellation = floe_execution::Cancellation::default();
        let connection = fixture
            .core
            .calendar_connection(fixture.person_id)
            .await
            .unwrap()
            .unwrap();
        let blocking = SourceAccessRequirement::try_new(
            "floe.source.calendar",
            Some(ConnectorId::try_new("calendar.event_kit").unwrap()),
            Some(ConnectionId::try_new("fixture-connection").unwrap()),
            GrantOperation::Read,
            GrantConsumer::builtin("assistant").unwrap(),
            GrantPurpose::Assistant,
            vec![ResourceHandle::try_new("home").unwrap()],
            None,
            SourceAccessRequirementKind::EnableObserve,
            Some(connection.source_authority),
            None,
            true,
        )
        .unwrap();
        let snapshot = snapshots
            .capture_inline(&blocking, fixture.person_id, "device", &cancellation)
            .await
            .unwrap();
        assert_eq!(snapshot.members.len(), 1);
        assert_eq!(snapshot.members[0].member_id, "calendar.timeline");
        assert_eq!(snapshot.members[0].resource, "home");
        assert_eq!(snapshot.members[0].expected_grant, None);
        assert_eq!(snapshot.connection_revision, Some(connection.revision));
        assert_eq!(
            snapshot.native_subject.as_deref(),
            Some("d".repeat(64).as_str())
        );

        // A calendar outside the live selection cannot bind inline review.
        let foreign = SourceAccessRequirement::try_new(
            blocking.source_id(),
            blocking.connector_id().cloned(),
            blocking.connection_id().cloned(),
            GrantOperation::Read,
            GrantConsumer::builtin("assistant").unwrap(),
            GrantPurpose::Assistant,
            vec![ResourceHandle::try_new("elsewhere").unwrap()],
            None,
            SourceAccessRequirementKind::EnableObserve,
            Some(connection.source_authority),
            None,
            true,
        )
        .unwrap();
        assert!(
            snapshots
                .capture_inline(&foreign, fixture.person_id, "device", &cancellation)
                .await
                .is_err()
        );
    }
}
