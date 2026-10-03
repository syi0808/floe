use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};

/// Non-secret identity of the exact verified Gateway credential generation.
/// This value is an expectation; only a fresh GatewayAdmission confers admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedGatewayBinding {
    pub person_id: String,
    pub device_id: String,
    pub client_id: String,
    pub producer_instance: String,
    pub producer_key_fingerprint: String,
    pub producer_audience: String,
    pub enrollment_id: String,
    pub credential_generation: u64,
}

impl VerifiedGatewayBinding {
    pub fn binding_digest(&self) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(serde_json::to_vec(self).expect("verified Gateway identity serialization")).into()
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.credential_generation == 0
            || [&self.person_id, &self.device_id, &self.client_id, &self.producer_instance,
                &self.producer_audience, &self.enrollment_id].iter().any(|value|
                    value.is_empty() || value.len() > 256 || value.trim() != value.as_str()
                        || value.chars().any(char::is_control))
            || self.producer_key_fingerprint.len() != 64
            || !self.producer_key_fingerprint.bytes().all(|byte|
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }
}

pub trait GatewayAdmission: Send + Sync {
    fn admit<'a>(&'a self, expected: &'a VerifiedGatewayBinding, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<VerifiedGatewayBinding, AgentFailure>>;
}

impl<T: GatewayAdmission + ?Sized> GatewayAdmission for std::sync::Arc<T> {
    fn admit<'a>(&'a self, expected: &'a VerifiedGatewayBinding, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<VerifiedGatewayBinding, AgentFailure>> {
        (**self).admit(expected, scope)
    }
}
impl<T: GatewayAdmission + ?Sized> GatewayAdmission for &T {
    fn admit<'a>(&'a self, expected: &'a VerifiedGatewayBinding, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<VerifiedGatewayBinding, AgentFailure>> {
        (**self).admit(expected, scope)
    }
}
