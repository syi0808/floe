use floe_context_contract::{DataClass, GrantConsumer, GrantDataCategory, GrantPurpose};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedViewCapability {
    pub view_id: String,
    pub data_class: DataClass,
    pub categories: Vec<GrantDataCategory>,
    pub purposes: Vec<GrantPurpose>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedConsumerRegistration {
    pub package_identity: String,
    pub manifest_revision: u64,
    pub declared_view_capabilities: Vec<TrustedViewCapability>,
    pub consumer_identity: GrantConsumer,
}
/// Construction copies validated registration values. No callbacks into other owners.
pub trait TrustedConsumerCatalog: Send + Sync {
    fn registrations(&self) -> &[TrustedConsumerRegistration];
}
