//! Canonical remote ConnectionObserve: review the bundle, then enable it.
//!
//! Enabling Observe over a remote connection mutates authority, so it binds
//! the exact bundle the Person reviewed: every canonical policy view with
//! its live descriptor fields, grant expectation (or reviewed absence) and
//! recorded policy. The owner re-probes live and compares every field
//! inside the existing atomic activation; a changed member, a new or
//! duplicate member, or a live grant outside the reviewed set refuses the
//! enable instead of widening it.
//!
//! Three commands share this module: `ConnectionObserveReview` reads the
//! reviewable snapshot without mutating, `ConnectionObserve` with
//! `enabled: true` echoes it back to enable, and `enabled: false`
//! pauses or disconnects without expectations (the narrowing direction
//! needs none). Plain inspect stays a cheap local status read.

use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

/// Everything a remote Observe review or enable judges.
pub(crate) struct RemoteObserveContext<'a, Keys: VaultKeyProvider> {
    pub core: &'a crate::FloeCore,
    pub vault: &'a EncryptedAgentVault<Keys>,
    pub person_id: PersonId,
    pub pairing: floe_access::RemotePairingIdentity<'a>,
    pub connector_id: &'a str,
    pub connection_id: &'a str,
    pub resource: Option<&'a str>,
    pub window: &'a floe_access::RemoteCallWindow,
}

/// The command identity a review or enable names.
pub(crate) fn validate_observe_identity(
    connector_id: &str,
    connection_id: &str,
    resource: Option<&str>,
) -> Result<(), AgentFailure> {
    if connector_id.trim().is_empty()
        || connector_id.len() > 256
        || connection_id.trim().is_empty()
        || connection_id.len() > 256
        || resource.is_some_and(|resource| resource.trim().is_empty() || resource.len() > 256)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

/// The reviewed bundle shape: 1-8 members in canonical view order, coherent
/// grant/policy pairs, valid authorities, no blank identifiers.
pub(crate) fn validate_observe_expectation(
    expected: &crate::RemoteConnectionObserveExpectation,
) -> Result<(), AgentFailure> {
    if expected.members.is_empty() || expected.members.len() > 8 {
        return Err(AgentFailure::InvalidInput);
    }
    let mut previous: Option<&str> = None;
    for member in &expected.members {
        if member.policy_fingerprint.len() != 64
            || !member
                .policy_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AgentFailure::InvalidInput);
        }
        for value in [
            &member.view_id,
            &member.resource,
            &member.producer_fingerprint,
            &member.provider_identity,
            &member.recipient,
        ] {
            if value.trim() != value.as_str()
                || value.is_empty()
                || value.len() > 256
                || value.chars().any(char::is_control)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if !member.source_authority.is_valid()
            || member
                .connection_revision
                .is_some_and(|revision| revision == 0)
        {
            return Err(AgentFailure::InvalidInput);
        }
        match (
            &member.expected_grant_id,
            &member.expected_grant_authority,
            &member.expected_policy,
        ) {
            (None, None, None) => {}
            (Some(id), Some(authority), Some(policy))
                if id.is_valid() && authority.is_valid() && policy.is_valid() => {}
            _ => return Err(AgentFailure::InvalidInput),
        }
        if previous.is_some_and(|previous| previous >= member.view_id.as_str()) {
            return Err(AgentFailure::InvalidInput);
        }
        previous = Some(member.view_id.as_str());
    }
    Ok(())
}

/// Read the reviewable bundle without mutating: probe each canonical member
/// and attach the live grant (or reviewed absence) and recorded policy.
/// Duplicate live authority for one member fails instead of presenting an
/// ambiguous snapshot.
pub(crate) async fn review_bundle<Keys, Transport>(
    ctx: &RemoteObserveContext<'_, Keys>,
    transport: &Transport,
) -> Result<crate::RemoteConnectionObserveExpectation, AgentFailure>
where
    Keys: VaultKeyProvider,
    Transport: floe_access::RemoteGrantTransport,
{
    validate_observe_identity(ctx.connector_id, ctx.connection_id, ctx.resource)?;
    if ctx.vault.person_id() != ctx.person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let policies = crate::first_party_observe::remote_policies(ctx.connector_id)?;
    if policies.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    let mut members = Vec::with_capacity(policies.len());
    for policy in &policies {
        members.push(review_member(ctx, transport, policy).await?);
    }
    members.sort_by(|left: &crate::RemoteObserveMemberExpectation, right| {
        left.view_id.cmp(&right.view_id)
    });
    let expected = crate::RemoteConnectionObserveExpectation { members };
    validate_observe_expectation(&expected)?;
    Ok(expected)
}

/// Enable the reviewed bundle: the reviewed set must exactly equal the
/// canonical policy views, every live grant must be covered by the review,
/// and each member re-probes live against its reviewed fields before the
/// one atomic activation commits.
pub(crate) async fn enable_bundle<Keys, Transport>(
    ctx: &RemoteObserveContext<'_, Keys>,
    transport: &Transport,
    expected: &crate::RemoteConnectionObserveExpectation,
) -> Result<(), AgentFailure>
where
    Keys: VaultKeyProvider,
    Transport: floe_access::RemoteGrantTransport,
{
    validate_observe_identity(ctx.connector_id, ctx.connection_id, ctx.resource)?;
    validate_observe_expectation(expected)?;
    if ctx.vault.person_id() != ctx.person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let policies = crate::first_party_observe::remote_policies(ctx.connector_id)?;
    if policies.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    let mut policy_views: Vec<&str> = policies.iter().map(|policy| policy.view_id).collect();
    policy_views.sort();
    let mut reviewed_views: Vec<&str> = expected
        .members
        .iter()
        .map(|member| member.view_id.as_str())
        .collect();
    reviewed_views.sort();
    if policy_views != reviewed_views {
        return Err(AgentFailure::InvalidInput);
    }
    for policy in &policies {
        let reviewed = expected
            .members
            .iter()
            .find(|member| member.view_id == policy.view_id)
            .ok_or(AgentFailure::InvalidInput)?;
        if reviewed.policy_fingerprint != crate::first_party_observe::policy_fingerprint(policy)? {
            return Err(AgentFailure::AccessReviewRequired);
        }
    }
    let calendar = policies.len() == 1 && policies[0].view_id == "calendar.timeline";
    if calendar != (ctx.resource.is_some()) {
        return Err(AgentFailure::InvalidInput);
    }
    // No live grant outside the reviewed set: an unreviewed grant refuses
    // the enable instead of being silently revoked or adopted.
    for member in &expected.members {
        let live = live_member_grants(
            ctx.vault,
            ctx.person_id,
            ctx.connector_id,
            ctx.connection_id,
            &member.resource,
        )
        .await?;
        match (&member.expected_grant_id, live.as_slice()) {
            (None, []) => {}
            (Some(id), [grant]) if grant.id() == *id => {}
            _ => return Err(AgentFailure::AccessReviewRequired),
        }
        if calendar {
            let resource = ctx.resource.ok_or(AgentFailure::InvalidInput)?;
            if member.view_id != "calendar.timeline" || member.resource != resource {
                return Err(AgentFailure::InvalidInput);
            }
            let live_revision = local_calendar_revision(ctx).await?;
            if member
                .connection_revision
                .is_some_and(|revision| revision != live_revision)
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
        } else if !floe_context::is_remote_view(&member.view_id)
            || member.resource
                != floe_context::remote_view_resource(&member.view_id, ctx.connection_id)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    let mut activations = Vec::new();
    for policy in &policies {
        let member = expected
            .members
            .iter()
            .find(|member| member.view_id == policy.view_id)
            .ok_or(AgentFailure::InvalidInput)?;
        if calendar {
            enable_calendar_member(ctx, transport, policy, member).await?;
        } else if let Some(activation) = prepare_view_member(ctx, transport, policy, member).await?
        {
            activations.push(activation);
        }
    }
    if !activations.is_empty() {
        ctx.vault.activate_remote_view_grants(activations).await?;
    }
    Ok(())
}

/// Pause or disconnect without expectations: narrowing authority needs no
/// review, and every mutation still CASes the authority it read. The
/// narrowing match is connection-wide on purpose: disconnect tears the
/// whole connection down, and pause keeps its established scope.
pub(crate) async fn disable_bundle<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
    resource: Option<&str>,
    disconnecting: bool,
) -> Result<(), AgentFailure> {
    validate_observe_identity(connector_id, connection_id, resource)?;
    if vault.person_id() != person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let policies = crate::first_party_observe::remote_policies(connector_id)?;
    if policies.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    let expected_views: Vec<&str> = policies.iter().map(|policy| policy.view_id).collect();
    for grant in vault.list_data_access_grants(128).await? {
        if grant.source().connector().as_str() == connector_id
            && grant.source().connection_id().as_str() == connection_id
            && grant.state() == floe_access::GrantState::Active
            && (grant.scope().resources().iter().any(|value| {
                expected_views.iter().any(|view| {
                    value.as_str() == floe_context::remote_view_resource(view, connection_id)
                })
            }) || expected_views == ["calendar.timeline"])
        {
            if disconnecting {
                vault
                    .revoke_data_access_grant(grant.id(), grant.authority())
                    .await?;
            } else if expected_views == ["calendar.timeline"] {
                vault
                    .pause_remote_calendar_grant(grant.id(), grant.authority())
                    .await?;
            } else {
                vault
                    .pause_remote_view_grant(grant.id(), grant.authority())
                    .await?;
            }
        }
    }
    Ok(())
}

/// The cheap local status read inspect serves: no probe, no mutation.
pub(crate) async fn observe_status<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
    resource: Option<&str>,
) -> Result<String, AgentFailure> {
    validate_observe_identity(connector_id, connection_id, resource)?;
    if vault.person_id() != person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let policies = crate::first_party_observe::remote_policies(connector_id)?;
    if policies.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    let expected_views: Vec<&str> = policies.iter().map(|policy| policy.view_id).collect();
    let grants = vault.list_data_access_grants(128).await?;
    let relevant = grants
        .iter()
        .filter(|grant| {
            grant.source().person_id() == person_id
                && grant.source().connector().as_str() == connector_id
                && grant.source().connection_id().as_str() == connection_id
                && grant.state() != floe_access::GrantState::Revoked
                && (expected_views == ["calendar.timeline"]
                    || grant.scope().resources().iter().any(|value| {
                        expected_views.iter().any(|view| {
                            value.as_str()
                                == floe_context::remote_view_resource(view, connection_id)
                        })
                    }))
        })
        .collect::<Vec<_>>();
    let status = if relevant.len() != policies.len() {
        "needs_review"
    } else if relevant.iter().all(|grant| {
        grant.state() == floe_access::GrantState::Active
            && !grant.review_required()
            && policies.iter().any(|policy| {
                let mut expected = policy.consumers.clone();
                expected.sort();
                let mut actual = grant.scope().consumers().to_vec();
                actual.sort();
                expected == actual
            })
    }) {
        "active"
    } else if relevant
        .iter()
        .all(|grant| grant.state() == floe_access::GrantState::Paused)
    {
        "paused"
    } else {
        "needs_review"
    };
    Ok(status.to_owned())
}

async fn review_member<Keys, Transport>(
    ctx: &RemoteObserveContext<'_, Keys>,
    transport: &Transport,
    policy: &crate::first_party_observe::FirstPartyObservePolicy,
) -> Result<crate::RemoteObserveMemberExpectation, AgentFailure>
where
    Keys: VaultKeyProvider,
    Transport: floe_access::RemoteGrantTransport,
{
    let calendar = policy.view_id == "calendar.timeline";
    if calendar {
        let resource = ctx.resource.ok_or(AgentFailure::InvalidInput)?;
        return review_calendar_member(ctx, transport, policy, resource).await;
    }
    if ctx.resource.is_some() || !floe_context::is_remote_view(policy.view_id) {
        return Err(AgentFailure::InvalidInput);
    }
    let resource = floe_context::remote_view_resource(policy.view_id, ctx.connection_id);
    let consumers = policy.consumers.clone();
    let request = floe_access::RemoteViewGrantRequest {
        person_id: ctx.person_id,
        pairing: ctx.pairing,
        view_id: policy.view_id,
        connector_id: ctx.connector_id,
        connection_id: ctx.connection_id,
        resource: &resource,
        consumers: &consumers,
        data_category: policy
            .categories
            .first()
            .cloned()
            .ok_or(AgentFailure::InvalidInput)?,
    };
    let preview = floe_access::preview_remote_view_grant(
        ctx.vault, transport, request, true, true, ctx.window,
    )
    .await?;
    let live = live_member_grants(
        ctx.vault,
        ctx.person_id,
        ctx.connector_id,
        ctx.connection_id,
        &resource,
    )
    .await?;
    let (expected_grant_id, expected_grant_authority, expected_policy) = match live.as_slice() {
        [] => (None, None, None),
        [grant] => {
            let (recorded, policy) = ctx
                .vault
                .remote_view_grant_policy(
                    policy.view_id,
                    ctx.connector_id,
                    ctx.connection_id,
                    grant.source().source_authority(),
                )
                .await?;
            if recorded != grant.id() {
                return Err(AgentFailure::AccessReviewRequired);
            }
            (Some(grant.id()), Some(grant.authority()), Some(policy))
        }
        _ => return Err(AgentFailure::Conflict),
    };
    Ok(crate::RemoteObserveMemberExpectation {
        view_id: policy.view_id.to_owned(),
        policy_fingerprint: crate::first_party_observe::policy_fingerprint(policy)?,
        resource,
        producer_fingerprint: preview.producer.fingerprint,
        source_authority: preview.reference.source_authority,
        connection_revision: Some(preview.connection_revision),
        provider_identity: preview.reference.provider_identity,
        recipient: preview.producer.audience,
        expected_grant_id,
        expected_grant_authority,
        expected_policy,
    })
}

async fn review_calendar_member<Keys, Transport>(
    ctx: &RemoteObserveContext<'_, Keys>,
    transport: &Transport,
    policy: &crate::first_party_observe::FirstPartyObservePolicy,
    resource: &str,
) -> Result<crate::RemoteObserveMemberExpectation, AgentFailure>
where
    Keys: VaultKeyProvider,
    Transport: floe_access::RemoteGrantTransport,
{
    let consumers = policy.consumers.clone();
    let evidence =
        super::calendar_access::remote_calendar_evidence(ctx.core, ctx.person_id).await?;
    let request = floe_access::RemoteCalendarGrantRequest {
        person_id: ctx.person_id,
        pairing: ctx.pairing,
        connector_id: ctx.connector_id,
        connection_id: ctx.connection_id,
        resource,
    };
    let preview = floe_access::preview_remote_calendar_grant(
        ctx.vault,
        transport,
        request,
        evidence.as_access(),
        &consumers,
        ctx.window,
    )
    .await?;
    Ok(crate::RemoteObserveMemberExpectation {
        view_id: policy.view_id.to_owned(),
        policy_fingerprint: crate::first_party_observe::policy_fingerprint(policy)?,
        resource: resource.to_owned(),
        producer_fingerprint: preview.producer.fingerprint,
        source_authority: preview.reference.source_authority,
        connection_revision: Some(local_calendar_revision(ctx).await?),
        provider_identity: preview.reference.provider_identity,
        recipient: preview.recipient,
        expected_grant_id: preview.grant_id,
        expected_grant_authority: preview.grant_authority,
        expected_policy: preview.consumer_policy,
    })
}

async fn prepare_view_member<Keys, Transport>(
    ctx: &RemoteObserveContext<'_, Keys>,
    transport: &Transport,
    policy: &crate::first_party_observe::FirstPartyObservePolicy,
    member: &crate::RemoteObserveMemberExpectation,
) -> Result<Option<floe_access::RemoteViewGrantActivation>, AgentFailure>
where
    Keys: VaultKeyProvider,
    Transport: floe_access::RemoteGrantTransport,
{
    let consumers = policy.consumers.clone();
    let request = floe_access::RemoteViewGrantRequest {
        person_id: ctx.person_id,
        pairing: ctx.pairing,
        view_id: policy.view_id,
        connector_id: ctx.connector_id,
        connection_id: ctx.connection_id,
        resource: &member.resource,
        consumers: &consumers,
        data_category: policy
            .categories
            .first()
            .cloned()
            .ok_or(AgentFailure::InvalidInput)?,
    };
    let expected_grant = match (member.expected_grant_id, member.expected_grant_authority) {
        (Some(id), Some(authority)) => Some((id, authority)),
        (None, None) => None,
        _ => return Err(AgentFailure::InvalidInput),
    };
    match floe_access::prepare_remote_view_grant_activation(
        ctx.vault,
        transport,
        request,
        floe_access::RemoteViewGrantExpectation {
            producer_fingerprint: &member.producer_fingerprint,
            source_authority: member.source_authority,
            connection_revision: member.connection_revision,
            provider_identity: &member.provider_identity,
            recipient: &member.recipient,
            expected_grant,
            expected_policy: member.expected_policy,
        },
        true,
        true,
        ctx.window,
    )
    .await?
    {
        floe_access::RemoteViewGrantPreparation::Current(_) => Ok(None),
        floe_access::RemoteViewGrantPreparation::Activate(activation) => Ok(Some(activation)),
    }
}

async fn enable_calendar_member<Keys, Transport>(
    ctx: &RemoteObserveContext<'_, Keys>,
    transport: &Transport,
    policy: &crate::first_party_observe::FirstPartyObservePolicy,
    member: &crate::RemoteObserveMemberExpectation,
) -> Result<(), AgentFailure>
where
    Keys: VaultKeyProvider,
    Transport: floe_access::RemoteGrantTransport,
{
    let resource = ctx.resource.ok_or(AgentFailure::InvalidInput)?;
    let consumers = policy.consumers.clone();
    let evidence =
        super::calendar_access::remote_calendar_evidence(ctx.core, ctx.person_id).await?;
    floe_access::review_and_activate_remote_calendar_grant(
        ctx.vault,
        transport,
        floe_access::RemoteCalendarGrantRequest {
            person_id: ctx.person_id,
            pairing: ctx.pairing,
            connector_id: ctx.connector_id,
            connection_id: ctx.connection_id,
            resource,
        },
        evidence.as_access(),
        &consumers,
        floe_access::RemoteCalendarGrantReviewExpectation {
            producer_fingerprint: &member.producer_fingerprint,
            source_authority: member.source_authority,
            grant_id: member.expected_grant_id,
            grant_authority: member.expected_grant_authority,
            consumer_policy: member.expected_policy,
        },
        ctx.window,
    )
    .await?;
    Ok(())
}

async fn live_member_grants<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    connector_id: &str,
    connection_id: &str,
    resource: &str,
) -> Result<Vec<floe_access::DataAccessGrant>, AgentFailure> {
    super::review_snapshot::live_grants_for_member(
        vault,
        person_id,
        connector_id,
        connection_id,
        resource,
    )
    .await
}

async fn local_calendar_revision<Keys: VaultKeyProvider>(
    ctx: &RemoteObserveContext<'_, Keys>,
) -> Result<u64, AgentFailure> {
    let live = ctx
        .core
        .calendar_connection(ctx.person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if live.connection_id != ctx.connection_id
        || live.disconnected
        || live.revision == 0
        || !live.source_authority.is_valid()
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(live.revision)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use floe_access::{
        BoxFuture, RemoteCalendarQuery, RemoteGrantTransport, RemoteSourceQuery,
        SignedCalendarPreview, SignedSourcePreview,
    };
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory,
        GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction,
        ResourceHandle, SourceAuthority,
    };
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use sha2::{Digest, Sha256};
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct Keys {
        key: Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>,
    }

    impl floe_vault::VaultKeyProvider for Keys {
        fn load(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
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
            vault_id: Uuid,
            key: &floe_vault::VaultKey,
        ) -> Result<(), AgentFailure> {
            self.key
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    const CLIENT_ID: &str = "paired-client";
    const DEVICE_ID: &str = "test-device";

    struct ScriptedTransport {
        producer: floe_access::RemoteProducerIdentity,
        pkcs8: Vec<u8>,
        person_id: PersonId,
        connection_id: String,
        revision: Mutex<u64>,
        authority: Mutex<SourceAuthority>,
        provider_identity: Mutex<String>,
        view_probes: Mutex<u64>,
    }

    impl ScriptedTransport {
        fn sign(&self, descriptor: &[u8]) -> String {
            let pair = Ed25519KeyPair::from_pkcs8(&self.pkcs8).unwrap();
            let mut message = Vec::from(b"floe.remote.producer.v1\0".as_slice());
            message.extend_from_slice(descriptor);
            URL_SAFE_NO_PAD.encode(pair.sign(&message).as_ref())
        }

        fn view_preview(&self, view_id: &str, resource: &str) -> SignedSourcePreview {
            *self.view_probes.lock().unwrap() += 1;
            let authority = *self.authority.lock().unwrap();
            let revision = *self.revision.lock().unwrap();
            let descriptor = serde_json::json!({
                "v": 1,
                "operation": "remote_view_source_preview",
                "challenge_id": Uuid::new_v4().to_string(),
                "nonce": URL_SAFE_NO_PAD.encode([7u8; 32]),
                "view_id": view_id,
                "person_id": self.person_id.to_string(),
                "client_id": CLIENT_ID,
                "device_id": DEVICE_ID,
                "audience": self.producer.audience,
                "connector_id": "gmail",
                "connection_id": self.connection_id,
                "connection_revision": revision,
                "execution_owner": self.producer.execution_owner,
                "incarnation": authority.incarnation().to_string(),
                "epoch": authority.epoch().get(),
                "resource": resource,
                "provider_identity": self.provider_identity.lock().unwrap().clone(),
                "issued_at_unix_ms": 1_700_000_000_000i64,
            });
            let bytes = serde_json::to_vec(&descriptor).unwrap();
            SignedSourcePreview {
                descriptor_b64url: URL_SAFE_NO_PAD.encode(&bytes),
                producer_signature: self.sign(&bytes),
                connection_revision: revision,
                producer: self.producer.clone(),
            }
        }
    }

    impl RemoteGrantTransport for ScriptedTransport {
        fn producer_identity<'a>(
            &'a self,
            _window: &'a floe_access::RemoteCallWindow,
        ) -> BoxFuture<'a, Result<floe_access::RemoteProducerIdentity, AgentFailure>> {
            let producer = self.producer.clone();
            Box::pin(async move { Ok(producer) })
        }

        fn view_source_preview<'a>(
            &'a self,
            query: RemoteSourceQuery<'a>,
            _window: &'a floe_access::RemoteCallWindow,
        ) -> BoxFuture<'a, Result<SignedSourcePreview, AgentFailure>> {
            let preview = self.view_preview(query.view_id, query.resource);
            Box::pin(async move { Ok(preview) })
        }

        fn calendar_source_preview<'a>(
            &'a self,
            _query: RemoteCalendarQuery<'a>,
            _window: &'a floe_access::RemoteCallWindow,
        ) -> BoxFuture<'a, Result<SignedCalendarPreview, AgentFailure>> {
            Box::pin(async move { Err(AgentFailure::CapabilityUnavailable) })
        }
    }

    struct Fixture {
        core: crate::FloeCore,
        vault: EncryptedAgentVault<Keys>,
        person_id: PersonId,
        connection_id: String,
        transport: ScriptedTransport,
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
            let connection_id = Uuid::new_v4().to_string();
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
            let public = pair.public_key().as_ref().to_vec();
            let fingerprint = Sha256::digest(&public)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let instance_id = Uuid::new_v4().to_string();
            let producer = floe_access::RemoteProducerIdentity {
                schema_version: 1,
                audience: format!("floe.server:{instance_id}"),
                instance_id,
                execution_owner: Uuid::new_v4().to_string(),
                key_id: Uuid::new_v4().to_string(),
                public_key: URL_SAFE_NO_PAD.encode(&public),
                fingerprint,
            };
            vault.remote_pin_producer(producer.clone()).await.unwrap();
            let transport = ScriptedTransport {
                producer,
                pkcs8: pkcs8.as_ref().to_vec(),
                person_id,
                connection_id: connection_id.clone(),
                revision: Mutex::new(11),
                authority: Mutex::new(SourceAuthority::new()),
                provider_identity: Mutex::new("google:subject-a".into()),
                view_probes: Mutex::new(0),
            };
            Self {
                core,
                vault,
                person_id,
                connection_id,
                transport,
                _root: root,
            }
        }

        fn ctx<'a>(
            &'a self,
            window: &'a floe_access::RemoteCallWindow,
            pairing: floe_access::RemotePairingIdentity<'a>,
            resource: Option<&'a str>,
        ) -> RemoteObserveContext<'a, Keys> {
            RemoteObserveContext {
                core: &self.core,
                vault: &self.vault,
                person_id: self.person_id,
                pairing,
                connector_id: "gmail",
                connection_id: &self.connection_id,
                resource,
                window,
            }
        }

        fn pairing<'a>(&self, person: &'a str) -> floe_access::RemotePairingIdentity<'a> {
            floe_access::RemotePairingIdentity {
                person_id: person,
                client_id: CLIENT_ID,
                device_id: DEVICE_ID,
            }
        }

        async fn live_grants(&self) -> Vec<floe_access::DataAccessGrant> {
            self.vault
                .list_data_access_grants(128)
                .await
                .unwrap()
                .into_iter()
                .filter(|grant| grant.state() != floe_access::GrantState::Revoked)
                .collect()
        }
    }

    fn window(cancellation: &floe_execution::Cancellation) -> floe_access::RemoteCallWindow {
        floe_access::RemoteCallWindow {
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            cancellation: cancellation.clone(),
        }
    }

    #[tokio::test]
    async fn review_then_enable_binds_gmail_bundle_atomically() {
        let fixture = Fixture::open().await;
        let cancellation = floe_execution::Cancellation::default();
        let window = window(&cancellation);
        let person = fixture.person_id.to_string();
        let ctx = fixture.ctx(&window, fixture.pairing(&person), None);

        let reviewed = review_bundle(&ctx, &fixture.transport).await.unwrap();
        assert_eq!(reviewed.members.len(), 2);
        assert_eq!(reviewed.members[0].view_id, "life.logistics");
        assert_eq!(reviewed.members[1].view_id, "mail.communication");
        for member in &reviewed.members {
            assert_eq!(member.expected_grant_id, None);
            assert_eq!(member.connection_revision, Some(11));
        }
        assert_eq!(*fixture.transport.view_probes.lock().unwrap(), 2);

        enable_bundle(&ctx, &fixture.transport, &reviewed)
            .await
            .unwrap();
        let grants = fixture.live_grants().await;
        assert_eq!(grants.len(), 2);
        assert!(
            grants
                .iter()
                .all(|grant| grant.state() == floe_access::GrantState::Active)
        );
        let status = observe_status(
            &fixture.vault,
            fixture.person_id,
            "gmail",
            &fixture.connection_id,
            None,
        )
        .await
        .unwrap();
        assert_eq!(status, "active");
    }

    #[tokio::test]
    async fn stale_descriptor_refuses_without_mutation() {
        let fixture = Fixture::open().await;
        let cancellation = floe_execution::Cancellation::default();
        let window = window(&cancellation);
        let person = fixture.person_id.to_string();
        let ctx = fixture.ctx(&window, fixture.pairing(&person), None);
        let reviewed = review_bundle(&ctx, &fixture.transport).await.unwrap();

        // A moved source authority is not what was reviewed.
        *fixture.transport.authority.lock().unwrap() = SourceAuthority::new();
        assert_eq!(
            enable_bundle(&ctx, &fixture.transport, &reviewed).await,
            Err(AgentFailure::PolicyDenied)
        );
        assert!(fixture.live_grants().await.is_empty());

        // A moved server revision is not what was reviewed either.
        *fixture.transport.revision.lock().unwrap() = 12;
        let fresh = review_bundle(&ctx, &fixture.transport).await.unwrap();
        *fixture.transport.revision.lock().unwrap() = 13;
        assert_eq!(
            enable_bundle(&ctx, &fixture.transport, &fresh).await,
            Err(AgentFailure::PolicyDenied)
        );
        assert!(fixture.live_grants().await.is_empty());
    }

    #[tokio::test]
    async fn reviewed_set_mismatch_rejects_before_any_probe() {
        let fixture = Fixture::open().await;
        let cancellation = floe_execution::Cancellation::default();
        let window = window(&cancellation);
        let person = fixture.person_id.to_string();
        let ctx = fixture.ctx(&window, fixture.pairing(&person), None);
        let reviewed = review_bundle(&ctx, &fixture.transport).await.unwrap();
        *fixture.transport.view_probes.lock().unwrap() = 0;

        // A missing member, an extra member and a wrong resource all reject
        // before the owner is probed again.
        let mut missing = reviewed.clone();
        missing.members.pop();
        assert_eq!(
            enable_bundle(&ctx, &fixture.transport, &missing).await,
            Err(AgentFailure::InvalidInput)
        );
        let mut extra = reviewed.clone();
        extra.members.push(extra.members[0].clone());
        assert_eq!(
            enable_bundle(&ctx, &fixture.transport, &extra).await,
            Err(AgentFailure::InvalidInput)
        );
        let mut wrong_resource = reviewed.clone();
        wrong_resource.members[0].resource = "mail.communication:elsewhere".into();
        assert_eq!(
            enable_bundle(&ctx, &fixture.transport, &wrong_resource).await,
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(*fixture.transport.view_probes.lock().unwrap(), 0);
        assert!(fixture.live_grants().await.is_empty());
    }

    #[tokio::test]
    async fn concurrent_grant_conflicts_instead_of_silent_adoption() {
        let fixture = Fixture::open().await;
        let cancellation = floe_execution::Cancellation::default();
        let window = window(&cancellation);
        let person = fixture.person_id.to_string();
        let ctx = fixture.ctx(&window, fixture.pairing(&person), None);
        let reviewed = review_bundle(&ctx, &fixture.transport).await.unwrap();

        // A grant appears out of band for one reviewed-absent member.
        let authority = *fixture.transport.authority.lock().unwrap();
        let source = GrantSourceBinding::try_new(
            fixture.person_id,
            ConnectionId::try_new(fixture.connection_id.clone()).unwrap(),
            ConnectorId::try_new("gmail").unwrap(),
            ExecutionOwnerId::try_new(fixture.transport.producer.execution_owner.clone()).unwrap(),
            authority,
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new(reviewed.members[0].resource.clone()).unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("assistant").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let concurrent = fixture
            .vault
            .review_and_activate_remote_view_grant(
                &reviewed.members[0].view_id,
                floe_access::GrantId::new(),
                None,
                source,
                scope,
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            enable_bundle(&ctx, &fixture.transport, &reviewed).await,
            Err(AgentFailure::AccessReviewRequired)
        );
        // Nothing was activated, and the concurrent grant was not revoked:
        // the enable refuses instead of adopting or destroying it.
        let grants = fixture.live_grants().await;
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].id(), concurrent.id());
    }

    #[tokio::test]
    async fn already_granted_member_resolves_without_second_activation() {
        let fixture = Fixture::open().await;
        let cancellation = floe_execution::Cancellation::default();
        let window = window(&cancellation);
        let person = fixture.person_id.to_string();
        let ctx = fixture.ctx(&window, fixture.pairing(&person), None);
        let reviewed = review_bundle(&ctx, &fixture.transport).await.unwrap();
        enable_bundle(&ctx, &fixture.transport, &reviewed)
            .await
            .unwrap();
        let before = fixture.live_grants().await;

        // A fresh review binds the live grants; enabling again resolves
        // through the current path without advancing any authority.
        let again = review_bundle(&ctx, &fixture.transport).await.unwrap();
        assert!(
            again
                .members
                .iter()
                .all(|member| member.expected_grant_id.is_some())
        );
        enable_bundle(&ctx, &fixture.transport, &again)
            .await
            .unwrap();
        let after = fixture.live_grants().await;
        assert_eq!(before.len(), 2);
        assert_eq!(after.len(), 2);
        for grant in &before {
            let current = after.iter().find(|live| live.id() == grant.id()).unwrap();
            assert_eq!(current.authority(), grant.authority());
        }
    }

    #[tokio::test]
    async fn disable_still_pauses_and_disconnects_without_expectations() {
        let fixture = Fixture::open().await;
        let cancellation = floe_execution::Cancellation::default();
        let window = window(&cancellation);
        let person = fixture.person_id.to_string();
        let ctx = fixture.ctx(&window, fixture.pairing(&person), None);
        let reviewed = review_bundle(&ctx, &fixture.transport).await.unwrap();
        enable_bundle(&ctx, &fixture.transport, &reviewed)
            .await
            .unwrap();

        disable_bundle(
            &fixture.vault,
            fixture.person_id,
            "gmail",
            &fixture.connection_id,
            None,
            false,
        )
        .await
        .unwrap();
        assert!(
            fixture
                .live_grants()
                .await
                .iter()
                .all(|grant| grant.state() == floe_access::GrantState::Paused)
        );
        let status = observe_status(
            &fixture.vault,
            fixture.person_id,
            "gmail",
            &fixture.connection_id,
            None,
        )
        .await
        .unwrap();
        assert_eq!(status, "paused");

        disable_bundle(
            &fixture.vault,
            fixture.person_id,
            "gmail",
            &fixture.connection_id,
            None,
            false,
        )
        .await
        .unwrap();
        // Disconnect revokes active grants; paused ones stay paused.
        disable_bundle(
            &fixture.vault,
            fixture.person_id,
            "gmail",
            &fixture.connection_id,
            None,
            true,
        )
        .await
        .unwrap();
        assert_eq!(fixture.live_grants().await.len(), 2);

        let reviewed = review_bundle(&ctx, &fixture.transport).await.unwrap();
        enable_bundle(&ctx, &fixture.transport, &reviewed)
            .await
            .unwrap();
        disable_bundle(
            &fixture.vault,
            fixture.person_id,
            "gmail",
            &fixture.connection_id,
            None,
            true,
        )
        .await
        .unwrap();
        assert!(fixture.live_grants().await.is_empty());
    }
}
