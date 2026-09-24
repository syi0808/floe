//! What contextual recipient consent needs from outside Access.
//!
//! The consent store persists Access-owned consent records; connection
//! admission reloads the current saved pairing identity on every dispatch
//! check; the clock supplies wall time for bounded expiry. None of them
//! decides whether a dispatch is admissible.

use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;
use uuid::Uuid;

use crate::application::recipient_consent::RecipientConsent;
use crate::ports::remote_grants::BoxFuture;

/// Durable Access-owned recipient consents, keyed by content-derived id.
///
/// The store is dumb: Access validates, derives ids, and decides re-grant
/// semantics; the store persists, looks up, marks revoked, and prunes
/// expired rows.
pub trait RecipientConsentStore: Sync {
    fn grant_consent<'a>(
        &'a self,
        consent: RecipientConsent,
    ) -> BoxFuture<'a, Result<RecipientConsent, AgentFailure>>;

    fn find_consent<'a>(
        &'a self,
        consent_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<RecipientConsent>, AgentFailure>>;

    fn revoke_consent<'a>(&'a self, consent_id: Uuid) -> BoxFuture<'a, Result<(), AgentFailure>>;

    fn prune_expired<'a>(&'a self, now_unix_ms: i64) -> BoxFuture<'a, Result<u64, AgentFailure>>;
}

/// Non-secret pairing identity, reloaded on every dispatch check.
///
/// Carries person/device/client binding only. Credentials never leave the
/// saved store, and recorded global consent flags are never consulted: they
/// are not product authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedModelConnection {
    pub person_id: String,
    pub device_id: String,
    pub client_id: String,
}

/// The current saved pairing, admitted against the verified caller.
///
/// Called on every dispatch check, so removal, re-pairing, or identity
/// mismatch denies the very next fence. Any failure fails closed.
pub trait ModelConnectionAdmission: Send + Sync {
    fn admit(&self) -> Result<AdmittedModelConnection, AgentFailure>;
}

/// Wall clock for bounded consent expiry.
pub trait RecipientConsentClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Production wall clock.
pub struct SystemConsentClock;

impl RecipientConsentClock for SystemConsentClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl<T: RecipientConsentStore + ?Sized> RecipientConsentStore for &T {
    fn grant_consent<'a>(
        &'a self,
        consent: RecipientConsent,
    ) -> BoxFuture<'a, Result<RecipientConsent, AgentFailure>> {
        (*self).grant_consent(consent)
    }

    fn find_consent<'a>(
        &'a self,
        consent_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<RecipientConsent>, AgentFailure>> {
        (*self).find_consent(consent_id)
    }

    fn revoke_consent<'a>(&'a self, consent_id: Uuid) -> BoxFuture<'a, Result<(), AgentFailure>> {
        (*self).revoke_consent(consent_id)
    }

    fn prune_expired<'a>(&'a self, now_unix_ms: i64) -> BoxFuture<'a, Result<u64, AgentFailure>> {
        (*self).prune_expired(now_unix_ms)
    }
}

impl<T: RecipientConsentStore + Send + ?Sized> RecipientConsentStore for std::sync::Arc<T> {
    fn grant_consent<'a>(
        &'a self,
        consent: RecipientConsent,
    ) -> BoxFuture<'a, Result<RecipientConsent, AgentFailure>> {
        (**self).grant_consent(consent)
    }

    fn find_consent<'a>(
        &'a self,
        consent_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<RecipientConsent>, AgentFailure>> {
        (**self).find_consent(consent_id)
    }

    fn revoke_consent<'a>(&'a self, consent_id: Uuid) -> BoxFuture<'a, Result<(), AgentFailure>> {
        (**self).revoke_consent(consent_id)
    }

    fn prune_expired<'a>(&'a self, now_unix_ms: i64) -> BoxFuture<'a, Result<u64, AgentFailure>> {
        (**self).prune_expired(now_unix_ms)
    }
}

impl<T: ModelConnectionAdmission + ?Sized> ModelConnectionAdmission for &T {
    fn admit(&self) -> Result<AdmittedModelConnection, AgentFailure> {
        (*self).admit()
    }
}

impl<T: ModelConnectionAdmission + ?Sized> ModelConnectionAdmission for std::sync::Arc<T> {
    fn admit(&self) -> Result<AdmittedModelConnection, AgentFailure> {
        (**self).admit()
    }
}

impl<T: RecipientConsentClock + ?Sized> RecipientConsentClock for &T {
    fn now(&self) -> DateTime<Utc> {
        (*self).now()
    }
}

impl<T: RecipientConsentClock + ?Sized> RecipientConsentClock for std::sync::Arc<T> {
    fn now(&self) -> DateTime<Utc> {
        (**self).now()
    }
}
