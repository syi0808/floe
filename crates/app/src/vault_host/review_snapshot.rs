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
//!   bundle members, the live native subject, the pinned producer and the
//!   local connection revision. A
//!   sibling live grant the requirement never named is reviewed as observed,
//!   never silently adopted later.
//!
//! Any capture failure downgrades the card to navigation-only: an unresolved
//! target with insufficient current identity gets navigation/review actions,
//! never an inline mutation it cannot bind.

use floe_access::{GrantAuthority, GrantId};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_context_contract::{SourceAccessRequirement, SourceAuthority};
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

/// One reviewed grant of the captured bundle.
#[derive(Clone, Debug)]
pub(crate) struct SnapshotMember {
    pub member_id: String,
    pub policy_digest: String,
    pub resource: String,
    /// The reviewed grant expectation: `None` is reviewed absence.
    pub expected_grant: Option<(GrantId, GrantAuthority)>,
}

/// The owner identity an inline Observe decision binds.
#[derive(Clone, Debug)]
pub(crate) struct InlineReviewSnapshot {
    pub source_revision: Option<SourceAuthority>,
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
    pub remote_source_client: Option<&'a floe_provider_adapters::sources::ServerSourceClient>,
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
                .capture_remote(requirement, person_id, connector, connection, cancellation)
                .await;
        }
        // Unknown connectors, query-bound feasibility and picker-owned
        // contacts selection cannot bind an inline mutation: navigate.
        Err(AgentFailure::CapabilityUnavailable)
    }

    /// Native Calendar reviews one logical View after probing every current
    /// Calendar resource under the local connection revision.
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
        let live = floe_context::CalendarConnectionReader::calendar_connection(
            &super::calendar_access::CoreCalendarConnections {
                core: self.core,
                person_id,
            },
        )
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
        if live.connection_id().as_str() != connection_id
            || live.execution_owner_id().as_str() != device_id
            || live.state() == floe_connections::SourceState::Disconnected
            || !live.source_authority().is_valid()
            || requirement.source_authority() != Some(live.source_authority())
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let calendar_ids: Vec<String> = live
            .resources()
            .iter()
            .map(|calendar| calendar.handle().as_str().to_owned())
            .collect();
        let logical_resource = floe_access::native_calendar_resource(connection_id)?;
        if requirement.resources() != [logical_resource.clone()] {
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
                provider: floe_context_contract::CalendarProvider::EventKit,
                device_id: device_id.to_owned(),
                calendar_ids,
                connection_scope: match live.resource_mode() {
                    floe_connections::ResourceMode::Selected => {
                        floe_context_contract::CalendarScope::Selected
                    }
                    floe_connections::ResourceMode::AllAvailable => {
                        floe_context_contract::CalendarScope::All
                    }
                },
                source_authority: requirement.source_authority(),
                reviewed_native_subject_fingerprint: None,
                connection_id: Some(live.connection_id().as_str().to_owned()),
            },
            &floe_access::RemoteCallWindow {
                deadline: self.capture_deadline,
                cancellation: cancellation.clone(),
            },
        )
        .await?;
        let connector = requirement
            .connector_id()
            .map(|id| id.as_str())
            .ok_or(AgentFailure::InvalidInput)?;
        let policy_digest = crate::first_party_observe::policy_digest(
            &crate::first_party_observe::calendar_policy()?,
        )?;
        let current = self
            .core
            .source_service()
            .update_native_subject(
                person_id,
                live.connection_id(),
                live.revision(),
                observation.native_subject_fingerprint.clone(),
            )
            .await
            .map_err(|_| AgentFailure::StaleContext)?;
        let expected = if current.source_authority() == live.source_authority() {
            observed_expectation(requirement)
        } else {
            None
        };
        self.verify_blocked_grant(
            person_id,
            connector,
            connection_id,
            logical_resource.as_str(),
            expected,
        )
        .await?;
        let members = vec![SnapshotMember {
            member_id: "calendar.timeline".to_owned(),
            policy_digest,
            resource: logical_resource.as_str().to_owned(),
            expected_grant: expected,
        }];
        Ok(InlineReviewSnapshot {
            source_revision: Some(current.source_authority()),
            members,
            connection_revision: Some(current.revision()),
            producer_fingerprint: None,
            native_subject: Some(observation.native_subject_fingerprint),
        })
    }

    /// Personal attention/wellbeing: one current Connections source and logical View.
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
        let spec = crate::personal_source_spec::PersonalSourceSpec::for_connector(connector)?;
        let probe = match connector {
            floe_access::ATTENTION_CONNECTOR => floe_access::PersonalSubjectProbe::Attention,
            floe_access::WELLBEING_CONNECTOR => floe_access::PersonalSubjectProbe::Wellbeing,
            _ => return Err(AgentFailure::CapabilityUnavailable),
        };
        let connection_id = requirement
            .connection_id()
            .ok_or(AgentFailure::InvalidInput)?;
        let source = self
            .core
            .source_service()
            .load(person_id, connection_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        spec.validate_connection(&source, device_id)?;
        let logical =
            floe_context_contract::connection_view_resource(spec.view, source.connection_id())
                .map_err(|_| AgentFailure::InvalidInput)?;
        if source.connector_id().as_str() != connector
            || requirement.resources() != [logical.clone()]
            || requirement.source_authority() != Some(source.source_authority())
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let evidence = self
            .personal_subject
            .inspect(
                person_id,
                device_id,
                probe,
                Some(
                    source
                        .native_subject_fingerprint()
                        .ok_or(AgentFailure::AccessReviewRequired)?
                        .to_owned(),
                ),
                Some(self.capture_deadline),
                cancellation.clone(),
            )
            .await?;
        if evidence.before != evidence.after
            || Some(evidence.before.as_str()) != source.native_subject_fingerprint()
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let current = self
            .core
            .source_service()
            .load(person_id, source.connection_id())
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        if current.source_authority() != source.source_authority()
            || current.native_subject_fingerprint() != source.native_subject_fingerprint()
            || current
                .resources()
                .iter()
                .map(|resource| resource.handle())
                .ne(source.resources().iter().map(|resource| resource.handle()))
            || !current.is_serving()
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let expected = observed_expectation(requirement);
        self.verify_blocked_grant(
            person_id,
            connector,
            source.connection_id().as_str(),
            logical.as_str(),
            expected,
        )
        .await?;
        Ok(InlineReviewSnapshot {
            source_revision: Some(current.source_authority()),
            members: vec![SnapshotMember {
                member_id: spec.view.to_owned(),
                policy_digest: crate::first_party_observe::policy_digest(
                    &crate::first_party_observe::personal_policy(connector)?,
                )?,
                resource: logical.as_str().to_owned(),
                expected_grant: expected,
            }],
            connection_revision: Some(current.revision()),
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
        cancellation: &floe_execution::Cancellation,
    ) -> Result<InlineReviewSnapshot, AgentFailure> {
        let requested: Vec<String> = requirement
            .resources()
            .iter()
            .map(|resource| resource.as_str().to_owned())
            .collect();
        let source_client = self
            .remote_source_client
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        let person_text = person_id.to_string();
        let window = floe_access::RemoteCallWindow {
            deadline: self.capture_deadline,
            cancellation: cancellation.clone(),
        };
        let context = super::remote_observe::RemoteObserveContext {
            vault: self.vault,
            person_id,
            pairing: floe_access::RemotePairingIdentity {
                person_id: &person_text,
                client_id: source_client.source().client_id(),
                device_id: source_client.source().device_id(),
            },
            connector_id: connector,
            connection_id: connection,
            resource: None,
            window: &window,
        };
        let transport =
            floe_provider_adapters::sources::AuthorizedSourceClient::new(source_client, self.vault);
        let reviewed = super::remote_observe::review_bundle(&context, &transport).await?;
        let source_revision = reviewed.members[0].source_authority;
        if requirement.source_authority() != Some(source_revision) {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let mut members = Vec::with_capacity(reviewed.members.len());
        for member in &reviewed.members {
            let expected = match (member.expected_grant_id, member.expected_grant_authority) {
                (Some(id), Some(authority)) => Some((id, authority)),
                (None, None) => None,
                _ => return Err(AgentFailure::AccessReviewRequired),
            };
            if requested.contains(&member.resource) && expected != observed_expectation(requirement)
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
            members.push(SnapshotMember {
                member_id: member.view_id.clone(),
                policy_digest: member.policy_digest.clone(),
                resource: member.resource.clone(),
                expected_grant: expected,
            });
        }
        for resource in &requested {
            if !members.iter().any(|member| &member.resource == resource) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
        }
        Ok(InlineReviewSnapshot {
            source_revision: Some(source_revision),
            members,
            connection_revision: reviewed.members[0].connection_revision,
            producer_fingerprint: Some(reviewed.members[0].producer_fingerprint.clone()),
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
        live_grants_for_member(self.vault, person_id, connector, connection, resource).await
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
}

/// The live non-revoked grants naming one bundle member resource:
/// the same filter capture, publication gating and decision-time
/// re-verification all share, so a grant observed by one path is observed
/// by all of them.
pub(super) async fn live_grants_for_member<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector: &str,
    connection: &str,
    resource: &str,
) -> Result<Vec<floe_access::DataAccessGrant>, AgentFailure> {
    Ok(vault
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
                remote_source_client: None,
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
        let source = fixture
            .core
            .source_service()
            .establish_reviewed_native(
                fixture.person_id,
                ConnectorId::try_new(floe_access::ATTENTION_CONNECTOR).unwrap(),
                ConnectionId::try_new("attention.macos.local").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new("macos:device").unwrap(),
                floe_connections::ResourceMode::AllAvailable,
                vec![
                    floe_connections::ConnectionResource::new(
                        ResourceHandle::try_new(floe_access::ATTENTION_RESOURCE).unwrap(),
                        "Attention".into(),
                    )
                    .unwrap(),
                ],
                "c".repeat(64),
            )
            .await
            .unwrap();
        let logical = floe_context_contract::connection_view_resource(
            floe_context_contract::ATTENTION_VIEW_ID,
            source.connection_id(),
        )
        .unwrap();
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
                &SourceAccessRequirement::try_new(
                    "floe.source.attention",
                    Some(source.connector_id().clone()),
                    Some(source.connection_id().clone()),
                    GrantOperation::Read,
                    GrantConsumer::builtin("assistant").unwrap(),
                    GrantPurpose::Assistant,
                    vec![logical.clone()],
                    None,
                    SourceAccessRequirementKind::EnableObserve,
                    Some(source.source_authority()),
                    None,
                    true,
                )
                .unwrap(),
                fixture.person_id,
                "device",
                &cancellation,
            )
            .await
            .unwrap();
        assert_eq!(snapshot.members.len(), 1);
        assert_eq!(
            snapshot.members[0].member_id,
            floe_context_contract::ATTENTION_VIEW_ID
        );
        assert_eq!(snapshot.members[0].resource, logical.as_str());
        assert_eq!(snapshot.members[0].expected_grant, None);
        assert_eq!(
            snapshot.native_subject.as_deref(),
            Some("c".repeat(64).as_str())
        );
        assert_eq!(snapshot.connection_revision, Some(source.revision()));
        assert_eq!(snapshot.source_revision, Some(source.source_authority()));
        assert_eq!(snapshot.producer_fingerprint, None);
    }

    #[tokio::test]
    async fn native_calendar_capture_binds_revision_subject_and_selection() {
        let fixture = Fixture::open().await;
        fixture
            .core
            .source_service()
            .establish(
                fixture.person_id,
                ConnectorId::try_new("calendar.event_kit").unwrap(),
                ConnectionId::try_new("fixture-connection").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new("device").unwrap(),
                floe_connections::ResourceMode::Selected,
                vec![
                    floe_connections::ConnectionResource::new(
                        ResourceHandle::try_new("home").unwrap(),
                        "Home".into(),
                    )
                    .unwrap(),
                ],
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
            .source_service()
            .load(
                fixture.person_id,
                &ConnectionId::try_new("fixture-connection").unwrap(),
            )
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
            vec![floe_access::native_calendar_resource("fixture-connection").unwrap()],
            None,
            SourceAccessRequirementKind::EnableObserve,
            Some(connection.source_authority()),
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
        assert_eq!(
            snapshot.members[0].resource,
            "calendar.timeline:fixture-connection"
        );
        assert_eq!(snapshot.members[0].expected_grant, None);
        assert_eq!(
            snapshot.connection_revision,
            Some(connection.revision() + 1)
        );
        let current = fixture
            .core
            .source_service()
            .load(
                fixture.person_id,
                &ConnectionId::try_new("fixture-connection").unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            current.native_subject_fingerprint(),
            Some("d".repeat(64).as_str())
        );
        assert_eq!(snapshot.source_revision, Some(current.source_authority()));
        assert_eq!(
            snapshot.native_subject.as_deref(),
            Some("d".repeat(64).as_str())
        );

        // A leaf resource cannot bind the logical inline review.
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
            Some(connection.source_authority()),
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
