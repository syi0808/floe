//! Access-owned contextual recipient consent for model dispatch.
//!
//! A consent authorizes one exact external recipient for one reviewed
//! route/profile, purpose/consumer, input data classes, and source scope,
//! bound to the person, device, paired connection, and origin intent lineage
//! the person reviewed. It never changes source ProcessingRestriction, Act,
//! or saved global allow flags, and it never authorizes a different
//! recipient, profile, scope, lineage, device, or pairing.
//!
//! Consent identity is deterministic in the reviewed content: the same review
//! grants the same consent id, so replays and response-loss retries rejoin
//! instead of duplicating. A bounded 24-hour time-to-live matches the
//! interaction pending-review lifetime; denial, revocation, and expiry never
//! become permanent global choices.

use chrono::{DateTime, Duration, Utc};
use floe_context_contract::{
    DataClass, GrantValidationError, ProcessingSourceScope, RecipientLineage,
};
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ports::model_dispatch::{
    ModelDispatchRecipientAuthority, ModelDispatchRequest, RecipientCheckOutcome,
};
use crate::ports::recipient_consent::{
    ModelConnectionAdmission, RecipientConsentClock, RecipientConsentStore,
};

/// Bounded consent lifetime: 24 hours from the review, matching the
/// interaction pending-review lifetime. Survives a crash between approval
/// and linked resume; never a standing authorization.
pub const RECIPIENT_CONSENT_TTL: Duration = Duration::hours(24);
pub const MAX_CONSENT_DEVICE_BYTES: usize = 256;
pub const MAX_CONSENT_CLIENT_BYTES: usize = 128;

/// Fixed namespace for deterministic consent identity.
pub const RECIPIENT_CONSENT_NAMESPACE: Uuid =
    Uuid::from_u128(0x7a1c_9e4b_2f6d_4a8e_b3c5_d7e9f1a2b4c6);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipientConsentState {
    Active,
    Revoked,
}

/// One durable contextual consent.
///
/// `projection_ref`/`projection_revision` preserve the original
/// projection/attempt identity for audit only; usable consent is never bound
/// to them, because a fresh resume re-observes. Matching compares the
/// reviewed exact recipient, profile, purpose/consumer, data classes, source
/// scopes, lineage, and person/device/pairing binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipientConsent {
    id: Uuid,
    person_id: PersonId,
    device_id: String,
    client_id: String,
    recipient: String,
    profile_id: String,
    purpose: String,
    consumer: String,
    input_data_classes: Vec<DataClass>,
    source_scopes: Vec<ProcessingSourceScope>,
    lineage: RecipientLineage,
    projection_ref: Uuid,
    projection_revision: u64,
    revision: u64,
    state: RecipientConsentState,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl RecipientConsent {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        person_id: PersonId,
        device_id: impl Into<String>,
        client_id: impl Into<String>,
        recipient: impl Into<String>,
        profile_id: impl Into<String>,
        purpose: impl Into<String>,
        consumer: impl Into<String>,
        mut input_data_classes: Vec<DataClass>,
        mut source_scopes: Vec<ProcessingSourceScope>,
        lineage: RecipientLineage,
        projection_ref: Uuid,
        projection_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<Self, GrantValidationError> {
        input_data_classes.sort();
        source_scopes.sort_by_cached_key(|scope| serde_json::to_vec(scope).unwrap_or_default());
        let device_id = device_id.into();
        let client_id = client_id.into();
        let recipient = recipient.into();
        let profile_id = profile_id.into();
        let purpose = purpose.into();
        let consumer = consumer.into();
        let id = recipient_consent_id(
            person_id,
            &device_id,
            &client_id,
            &recipient,
            &profile_id,
            &purpose,
            &consumer,
            &input_data_classes,
            &source_scopes,
            lineage,
        );
        let expires_at = now
            .checked_add_signed(RECIPIENT_CONSENT_TTL)
            .ok_or(GrantValidationError::InvalidState)?;
        let consent = Self {
            id,
            person_id,
            device_id,
            client_id,
            recipient,
            profile_id,
            purpose,
            consumer,
            input_data_classes,
            source_scopes,
            lineage,
            projection_ref,
            projection_revision,
            revision: 1,
            state: RecipientConsentState::Active,
            created_at: now,
            expires_at,
        };
        consent.validate()?;
        Ok(consent)
    }

    pub fn validate(&self) -> Result<(), GrantValidationError> {
        if !self.person_id.is_valid() {
            return Err(GrantValidationError::Identity);
        }
        validate_bounded(&self.device_id, MAX_CONSENT_DEVICE_BYTES, "device")?;
        validate_bounded(&self.client_id, MAX_CONSENT_CLIENT_BYTES, "client")?;
        floe_context_contract::ProcessingRequirement::try_new(
            self.recipient.clone(),
            self.profile_id.clone(),
            self.purpose.clone(),
            self.consumer.clone(),
            self.input_data_classes.clone(),
            self.source_scopes.clone(),
            self.projection_ref,
            self.projection_revision,
            self.lineage,
        )?;
        if self.revision == 0 {
            return Err(GrantValidationError::InvalidState);
        }
        let expected_expiry = self
            .created_at
            .checked_add_signed(RECIPIENT_CONSENT_TTL)
            .ok_or(GrantValidationError::InvalidState)?;
        if self.expires_at != expected_expiry {
            return Err(GrantValidationError::InvalidState);
        }
        let expected_id = recipient_consent_id(
            self.person_id,
            &self.device_id,
            &self.client_id,
            &self.recipient,
            &self.profile_id,
            &self.purpose,
            &self.consumer,
            &self.input_data_classes,
            &self.source_scopes,
            self.lineage,
        );
        if self.id != expected_id {
            return Err(GrantValidationError::InvalidState);
        }
        Ok(())
    }

    /// Whether this consent authorizes a dispatch right now: active, issued,
    /// and unexpired. A clock moved before issuance fails closed.
    pub fn is_usable_at(&self, now: DateTime<Utc>) -> bool {
        self.state == RecipientConsentState::Active
            && self.created_at <= now
            && now < self.expires_at
    }

    /// Mark revoked with a bumped revision. The content-bound id is
    /// unchanged: a later fresh review of identical content refreshes this
    /// same row.
    pub fn revoked(&self) -> Result<Self, GrantValidationError> {
        let revoked = Self {
            revision: self
                .revision
                .checked_add(1)
                .ok_or(GrantValidationError::InvalidState)?,
            state: RecipientConsentState::Revoked,
            ..self.clone()
        };
        revoked.validate()?;
        Ok(revoked)
    }

    /// A fresh review of the identical content after revoke/expiry: same
    /// content-bound id, bumped revision, new bounded window.
    pub fn refresh_for_regrant(&self, now: DateTime<Utc>) -> Result<Self, GrantValidationError> {
        let expires_at = now
            .checked_add_signed(RECIPIENT_CONSENT_TTL)
            .ok_or(GrantValidationError::InvalidState)?;
        let refreshed = Self {
            revision: self
                .revision
                .checked_add(1)
                .ok_or(GrantValidationError::InvalidState)?,
            state: RecipientConsentState::Active,
            created_at: now,
            expires_at,
            ..self.clone()
        };
        refreshed.validate()?;
        Ok(refreshed)
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    pub fn input_data_classes(&self) -> &[DataClass] {
        &self.input_data_classes
    }

    pub fn source_scopes(&self) -> &[ProcessingSourceScope] {
        &self.source_scopes
    }

    pub fn lineage(&self) -> RecipientLineage {
        self.lineage
    }

    pub fn projection_ref(&self) -> Uuid {
        self.projection_ref
    }

    pub fn projection_revision(&self) -> u64 {
        self.projection_revision
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn state(&self) -> RecipientConsentState {
        self.state
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

fn validate_bounded(
    value: &str,
    limit: usize,
    name: &'static str,
) -> Result<(), GrantValidationError> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        return Err(GrantValidationError::InvalidIdentifier(name));
    }
    Ok(())
}

/// Deterministic consent identity for the reviewed content.
///
/// Canonicalizes set-valued fields before hashing, so a fresh dispatch with
/// the same reviewed scope computes the same id as the granting review.
/// Timestamps, revision, state, and audit-only projection identity never
/// enter the id: replays, re-grants, and fresh resumes rejoin it.
#[allow(clippy::too_many_arguments)]
pub fn recipient_consent_id(
    person_id: PersonId,
    device_id: &str,
    client_id: &str,
    recipient: &str,
    profile_id: &str,
    purpose: &str,
    consumer: &str,
    input_data_classes: &[DataClass],
    source_scopes: &[ProcessingSourceScope],
    lineage: RecipientLineage,
) -> Uuid {
    let mut classes = input_data_classes.to_vec();
    classes.sort();
    let mut scopes = source_scopes.to_vec();
    scopes.sort_by_cached_key(|scope| serde_json::to_vec(scope).unwrap_or_default());
    let mut hasher = Sha256::new();
    hasher.update(b"floe.access.recipient-consent\0");
    hasher.update(person_id.as_uuid().as_bytes());
    for value in [
        device_id, client_id, recipient, profile_id, purpose, consumer,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    let classes_bytes = serde_json::to_vec(&classes).unwrap_or_default();
    hasher.update((classes_bytes.len() as u64).to_be_bytes());
    hasher.update(&classes_bytes);
    let scopes_bytes = serde_json::to_vec(&scopes).unwrap_or_default();
    hasher.update((scopes_bytes.len() as u64).to_be_bytes());
    hasher.update(&scopes_bytes);
    hasher.update(lineage.session_id().as_bytes());
    hasher.update(lineage.origin_run_id().as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    Uuid::new_v5(&RECIPIENT_CONSENT_NAMESPACE, &digest)
}

/// Grant the reviewed consent, or rejoin it.
///
/// Prunes expired consents, then upserts by content-derived id: an identical
/// usable consent rejoins without a new write; a revoked/expired identical
/// review refreshes into a new bounded window with a bumped revision.
pub async fn grant_recipient_consent(
    store: &impl RecipientConsentStore,
    consent: RecipientConsent,
) -> Result<RecipientConsent, AgentFailure> {
    consent.validate().map_err(|_| AgentFailure::InvalidInput)?;
    store
        .prune_expired(consent.created_at.timestamp_millis())
        .await?;
    if let Some(existing) = store.find_consent(consent.id).await? {
        existing
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if existing.person_id != consent.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        if existing.is_usable_at(consent.created_at) {
            return Ok(existing);
        }
        let refreshed = existing
            .refresh_for_regrant(consent.created_at)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        return store.grant_consent(refreshed).await;
    }
    store.grant_consent(consent).await
}

/// Revoke one consent. Idempotent when already revoked; a consent that never
/// existed is NotFound, never a silent success.
pub async fn revoke_recipient_consent(
    store: &impl RecipientConsentStore,
    consent_id: Uuid,
    person_id: PersonId,
) -> Result<(), AgentFailure> {
    if consent_id.is_nil() || !person_id.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let existing = store
        .find_consent(consent_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    existing
        .validate()
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if existing.person_id != person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    if existing.state == RecipientConsentState::Revoked {
        return Ok(());
    }
    store.revoke_consent(consent_id).await
}

/// The canonical exact-recipient authority: current pairing admission plus
/// current contextual consent lookup. No saved global flags, no standing
/// authorization.
///
/// Every check reloads the live pairing (person/device/client binding) and
/// looks up the content-derived consent id for this exact dispatch. A
/// usable match grants; anything else is missing (reviewable at Access
/// discretion) or a hard fail-closed denial. Store and pairing failures
/// always map to PolicyDenied so a revocation can never trigger a hidden
/// fallback to another recipient.
pub struct ContextualRecipientAuthority<Store, Admission, Clock> {
    store: Store,
    admission: Admission,
    clock: Clock,
}

impl<Store, Admission, Clock> ContextualRecipientAuthority<Store, Admission, Clock> {
    pub fn new(store: Store, admission: Admission, clock: Clock) -> Self {
        Self {
            store,
            admission,
            clock,
        }
    }
}

impl<Store, Admission, Clock> ModelDispatchRecipientAuthority
    for ContextualRecipientAuthority<Store, Admission, Clock>
where
    Store: RecipientConsentStore + Send,
    Admission: ModelConnectionAdmission,
    Clock: RecipientConsentClock,
{
    fn check_recipient<'a>(
        &'a self,
        request: &'a ModelDispatchRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<RecipientCheckOutcome, AgentFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let recipient = match &request.target {
                crate::ports::model_dispatch::ModelDispatchTarget::External { recipient } => {
                    recipient.clone()
                }
                crate::ports::model_dispatch::ModelDispatchTarget::Device => {
                    return Err(AgentFailure::InvalidInput);
                }
            };
            let Some(lineage) = request.lineage else {
                // No lineage, no review and no grant: fail closed.
                return Err(AgentFailure::PolicyDenied);
            };
            // The live pairing, admitted per check. Removal, re-pairing, or
            // identity mismatch denies the very next fence.
            let admitted = self
                .admission
                .admit()
                .map_err(|_| AgentFailure::PolicyDenied)?;
            if admitted.person_id != request.person_id.to_string() {
                return Err(AgentFailure::PolicyDenied);
            }
            let scopes = match &request.coverage {
                floe_context_contract::DependencyCoverage::Unknown
                | floe_context_contract::DependencyCoverage::Independent => vec![],
                floe_context_contract::DependencyCoverage::Dependent { dependencies } => {
                    dependencies
                        .iter()
                        .map(floe_context_contract::ProcessingSourceScope::from_dependency)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| AgentFailure::PolicyDenied)?
                }
            };
            let id = recipient_consent_id(
                request.person_id,
                &admitted.device_id,
                &admitted.client_id,
                &recipient,
                &request.profile_id,
                &request.purpose,
                &request.consumer,
                &request.input_data_classes,
                &scopes,
                lineage,
            );
            let found = self
                .store
                .find_consent(id)
                .await
                .map_err(|_| AgentFailure::PolicyDenied)?;
            let Some(consent) = found else {
                return Ok(RecipientCheckOutcome::Missing);
            };
            consent.validate().map_err(|_| AgentFailure::PolicyDenied)?;
            if consent.id() != id || !consent.is_usable_at(self.clock.now()) {
                return Ok(RecipientCheckOutcome::Missing);
            }
            Ok(RecipientCheckOutcome::Granted)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use floe_context_contract::RecipientLineage;
    use uuid::Uuid;

    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn consent_at(created: DateTime<Utc>) -> RecipientConsent {
        RecipientConsent::try_new(
            PersonId::new(),
            "device",
            "client",
            "model.example",
            "server-model",
            "everyday_assistance",
            "conversation.root",
            vec![DataClass::Personal],
            vec![],
            RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap(),
            Uuid::new_v4(),
            1,
            created,
        )
        .unwrap()
    }

    fn consent() -> RecipientConsent {
        consent_at(now())
    }

    struct MemoryStore {
        records: Mutex<HashMap<Uuid, RecipientConsent>>,
    }

    impl MemoryStore {
        fn new() -> Self {
            Self {
                records: Mutex::new(HashMap::new()),
            }
        }
    }

    impl RecipientConsentStore for MemoryStore {
        fn grant_consent<'a>(
            &'a self,
            consent: RecipientConsent,
        ) -> crate::ports::remote_grants::BoxFuture<'a, Result<RecipientConsent, AgentFailure>>
        {
            Box::pin(async move {
                consent.validate().map_err(|_| AgentFailure::InvalidInput)?;
                self.records
                    .lock()
                    .unwrap()
                    .insert(consent.id, consent.clone());
                Ok(consent)
            })
        }

        fn find_consent<'a>(
            &'a self,
            consent_id: Uuid,
        ) -> crate::ports::remote_grants::BoxFuture<
            'a,
            Result<Option<RecipientConsent>, AgentFailure>,
        > {
            Box::pin(async move { Ok(self.records.lock().unwrap().get(&consent_id).cloned()) })
        }

        fn revoke_consent<'a>(
            &'a self,
            consent_id: Uuid,
        ) -> crate::ports::remote_grants::BoxFuture<'a, Result<(), AgentFailure>> {
            Box::pin(async move {
                let mut records = self.records.lock().unwrap();
                let Some(existing) = records.get(&consent_id).cloned() else {
                    return Err(AgentFailure::NotFound);
                };
                let mut revoked = existing;
                revoked.state = RecipientConsentState::Revoked;
                revoked.revision += 1;
                revoked
                    .validate()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                records.insert(consent_id, revoked);
                Ok(())
            })
        }

        fn prune_expired<'a>(
            &'a self,
            now_unix_ms: i64,
        ) -> crate::ports::remote_grants::BoxFuture<'a, Result<u64, AgentFailure>> {
            Box::pin(async move {
                let mut records = self.records.lock().unwrap();
                let before = records.len();
                records.retain(|_, consent| consent.expires_at.timestamp_millis() > now_unix_ms);
                Ok((before - records.len()) as u64)
            })
        }
    }

    #[test]
    fn consent_identity_is_deterministic_in_reviewed_content() {
        let first = consent();
        // Audit-only projection identity and wall time never enter the id.
        let rebuilt = RecipientConsent::try_new(
            first.person_id,
            first.device_id.clone(),
            first.client_id.clone(),
            first.recipient.clone(),
            first.profile_id.clone(),
            first.purpose.clone(),
            first.consumer.clone(),
            first.input_data_classes.clone(),
            first.source_scopes.clone(),
            first.lineage,
            Uuid::new_v4(),
            9,
            now() + Duration::hours(1),
        )
        .unwrap();
        assert_eq!(rebuilt.id, first.id);
        assert_eq!(rebuilt.revision, 1);
    }

    #[test]
    fn changed_review_fields_change_consent_identity() {
        let base = consent();
        let variant = |mutate: fn(&mut RecipientConsent)| {
            let mut consent = base.clone();
            mutate(&mut consent);
            recipient_consent_id(
                consent.person_id,
                &consent.device_id,
                &consent.client_id,
                &consent.recipient,
                &consent.profile_id,
                &consent.purpose,
                &consent.consumer,
                &consent.input_data_classes,
                &consent.source_scopes,
                consent.lineage,
            )
        };
        assert_ne!(
            variant(|consent| consent.recipient = "other.example".into()),
            base.id
        );
        assert_ne!(
            variant(|consent| consent.profile_id = "other-model".into()),
            base.id
        );
        assert_ne!(
            variant(|consent| consent.device_id = "other-device".into()),
            base.id
        );
        assert_ne!(
            variant(|consent| consent.client_id = "other-client".into()),
            base.id
        );
        assert_ne!(
            variant(|consent| consent.lineage =
                RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap()),
            base.id
        );
    }

    #[test]
    fn usability_is_bounded_and_fails_closed_on_backwards_clock() {
        let consent = consent();
        assert!(!consent.is_usable_at(now() - Duration::seconds(1)));
        assert!(consent.is_usable_at(now()));
        assert!(consent.is_usable_at(now() + RECIPIENT_CONSENT_TTL - Duration::seconds(1)));
        assert!(!consent.is_usable_at(now() + RECIPIENT_CONSENT_TTL));
    }

    #[tokio::test]
    async fn grant_rejoins_regrants_and_revokes() {
        let store = MemoryStore::new();
        let review = consent();
        let granted = grant_recipient_consent(&store, review.clone())
            .await
            .unwrap();
        assert_eq!(granted.revision, 1);
        // Identical review rejoins without a new revision.
        let rejoined = grant_recipient_consent(&store, review.clone())
            .await
            .unwrap();
        assert_eq!(rejoined, granted);
        // Revoke ends it; revoke is idempotent.
        revoke_recipient_consent(&store, granted.id, granted.person_id)
            .await
            .unwrap();
        revoke_recipient_consent(&store, granted.id, granted.person_id)
            .await
            .unwrap();
        let revoked = store.find_consent(granted.id).await.unwrap().unwrap();
        assert_eq!(revoked.state, RecipientConsentState::Revoked);
        assert!(!revoked.is_usable_at(now() + Duration::seconds(1)));
        // A fresh review of identical content refreshes the same id.
        let rereview = RecipientConsent::try_new(
            review.person_id,
            review.device_id.clone(),
            review.client_id.clone(),
            review.recipient.clone(),
            review.profile_id.clone(),
            review.purpose.clone(),
            review.consumer.clone(),
            review.input_data_classes.to_vec(),
            review.source_scopes.to_vec(),
            review.lineage,
            Uuid::new_v4(),
            2,
            now() + Duration::seconds(1),
        )
        .unwrap();
        assert_eq!(rereview.id, granted.id);
        let refreshed = grant_recipient_consent(&store, rereview).await.unwrap();
        assert_eq!(refreshed.id, granted.id);
        assert_eq!(refreshed.revision, granted.revision + 2);
        assert!(refreshed.is_usable_at(now() + Duration::seconds(1)));
        // Unknown consent revocation is NotFound, never silent success.
        assert_eq!(
            revoke_recipient_consent(&store, Uuid::new_v4(), granted.person_id).await,
            Err(AgentFailure::NotFound)
        );
        // Foreign person cannot revoke.
        assert_eq!(
            revoke_recipient_consent(&store, granted.id, PersonId::new()).await,
            Err(AgentFailure::CapabilityDenied)
        );
    }

    #[tokio::test]
    async fn grant_prunes_expired_consents() {
        let store = MemoryStore::new();
        let stale = consent_at(now() - RECIPIENT_CONSENT_TTL - Duration::seconds(1));
        store.grant_consent(stale.clone()).await.unwrap();
        assert!(store.find_consent(stale.id).await.unwrap().is_some());
        grant_recipient_consent(&store, consent()).await.unwrap();
        assert!(store.find_consent(stale.id).await.unwrap().is_none());
    }

    struct FakeAdmission {
        admitted:
            Mutex<Result<crate::ports::recipient_consent::AdmittedModelConnection, AgentFailure>>,
    }

    impl FakeAdmission {
        fn live(person_id: PersonId) -> Self {
            Self {
                admitted: Mutex::new(Ok(
                    crate::ports::recipient_consent::AdmittedModelConnection {
                        person_id: person_id.to_string(),
                        device_id: "device".into(),
                        client_id: "client".into(),
                    },
                )),
            }
        }
    }

    impl crate::ports::recipient_consent::ModelConnectionAdmission for FakeAdmission {
        fn admit(
            &self,
        ) -> Result<crate::ports::recipient_consent::AdmittedModelConnection, AgentFailure>
        {
            self.admitted.lock().unwrap().clone()
        }
    }

    struct ManualClock {
        now: Mutex<DateTime<Utc>>,
    }

    impl ManualClock {
        fn at(now: DateTime<Utc>) -> Self {
            Self {
                now: Mutex::new(now),
            }
        }
    }

    impl crate::ports::recipient_consent::RecipientConsentClock for ManualClock {
        fn now(&self) -> DateTime<Utc> {
            *self.now.lock().unwrap()
        }
    }

    fn dispatch_request(
        consent: &RecipientConsent,
    ) -> crate::ports::model_dispatch::ModelDispatchRequest {
        crate::ports::model_dispatch::ModelDispatchRequest {
            person_id: consent.person_id(),
            projection_ref: Uuid::new_v4(),
            projection_revision: 7,
            coverage: floe_context_contract::DependencyCoverage::Independent,
            input_data_classes: consent.input_data_classes().to_vec(),
            purpose: consent.purpose().to_owned(),
            consumer: consent.consumer().to_owned(),
            profile_id: consent.profile_id().to_owned(),
            target: crate::ports::model_dispatch::ModelDispatchTarget::External {
                recipient: consent.recipient().to_owned(),
            },
            lineage: Some(consent.lineage()),
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            cancellation: floe_execution::Cancellation::default(),
        }
    }

    #[tokio::test]
    async fn authority_grants_matches_and_fails_closed() {
        let store = MemoryStore::new();
        let review = consent();
        let granted = grant_recipient_consent(&store, review.clone())
            .await
            .unwrap();
        let admission = FakeAdmission::live(granted.person_id());
        let clock = ManualClock::at(now());
        let authority = ContextualRecipientAuthority::new(&store, &admission, &clock);
        // The reviewed dispatch grants.
        assert_eq!(
            authority.check_recipient(&dispatch_request(&granted)).await,
            Ok(crate::ports::model_dispatch::RecipientCheckOutcome::Granted)
        );
        // A different recipient, profile, or lineage is missing, not granted.
        let mut other = dispatch_request(&granted);
        other.target = crate::ports::model_dispatch::ModelDispatchTarget::External {
            recipient: "other.example".into(),
        };
        assert_eq!(
            authority.check_recipient(&other).await,
            Ok(crate::ports::model_dispatch::RecipientCheckOutcome::Missing)
        );
        // No lineage fails closed without a reviewable outcome.
        let mut unlined = dispatch_request(&granted);
        unlined.lineage = None;
        assert_eq!(
            authority.check_recipient(&unlined).await,
            Err(AgentFailure::PolicyDenied)
        );
        // Re-pairing never matches the old review.
        *admission.admitted.lock().unwrap() =
            Ok(crate::ports::recipient_consent::AdmittedModelConnection {
                person_id: granted.person_id().to_string(),
                device_id: "device".into(),
                client_id: "repaired-client".into(),
            });
        assert_eq!(
            authority.check_recipient(&dispatch_request(&granted)).await,
            Ok(crate::ports::model_dispatch::RecipientCheckOutcome::Missing)
        );
        // A foreign pairing fails closed.
        *admission.admitted.lock().unwrap() =
            Ok(crate::ports::recipient_consent::AdmittedModelConnection {
                person_id: PersonId::new().to_string(),
                device_id: "device".into(),
                client_id: "client".into(),
            });
        assert_eq!(
            authority.check_recipient(&dispatch_request(&granted)).await,
            Err(AgentFailure::PolicyDenied)
        );
        // Pairing removal fails closed.
        *admission.admitted.lock().unwrap() = Err(AgentFailure::NotFound);
        assert_eq!(
            authority.check_recipient(&dispatch_request(&granted)).await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn authority_treats_revoked_and_expired_as_missing() {
        let store = MemoryStore::new();
        let review = consent();
        let granted = grant_recipient_consent(&store, review.clone())
            .await
            .unwrap();
        let admission = FakeAdmission::live(granted.person_id());
        let clock = ManualClock::at(now());
        let authority = ContextualRecipientAuthority::new(&store, &admission, &clock);
        revoke_recipient_consent(&store, granted.id(), granted.person_id())
            .await
            .unwrap();
        assert_eq!(
            authority.check_recipient(&dispatch_request(&granted)).await,
            Ok(crate::ports::model_dispatch::RecipientCheckOutcome::Missing)
        );
        // A fresh review of identical content grants again under the clock.
        let rereview = granted.refresh_for_regrant(now()).unwrap();
        grant_recipient_consent(&store, rereview).await.unwrap();
        assert_eq!(
            authority.check_recipient(&dispatch_request(&granted)).await,
            Ok(crate::ports::model_dispatch::RecipientCheckOutcome::Granted)
        );
        *clock.now.lock().unwrap() = now() + RECIPIENT_CONSENT_TTL;
        assert_eq!(
            authority.check_recipient(&dispatch_request(&granted)).await,
            Ok(crate::ports::model_dispatch::RecipientCheckOutcome::Missing)
        );
    }
}
