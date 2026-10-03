use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn valid_source_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestedProcessingDto {
    DeviceOnly,
    GatewayAllowed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ConnectionObserveSetMutationDto {
    #[serde(rename = "enable")]
    Enable {
        source_ref: super::ConnectionsSourceRefDto,
        review_ref: super::ReviewRefDto,
        expected_revision: u64,
    },
    #[serde(rename = "pause")]
    Pause {
        source_ref: super::ConnectionsSourceRefDto,
        expected_revision: u64,
    },
}

impl ConnectionObserveSetMutationDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Enable {
                review_ref,
                expected_revision,
                ..
            } => {
                validate_revision(*expected_revision)?;
                review_ref.validate()
            }
            Self::Pause {
                expected_revision, ..
            } => validate_revision(*expected_revision),
        }
    }
}

fn validate_revision(value: u64) -> Result<(), &'static str> {
    if value == 0 || value > i64::MAX as u64 {
        Err("expected_revision")
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySetupDto {
    pub target_ref: super::GatewaySetupRefDto,
    pub display_address: String,
    pub expires_at: String,
}

impl GatewaySetupDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_source_text(&self.display_address, 2048) || !valid_utc_timestamp(&self.expires_at) {
            return Err("connections.gateway_setup");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayStateDto {
    Unpaired,
    Paired,
    RepairRequired,
    Forgotten,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayActionDto {
    Pair,
    Forget,
    Manage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionsFailureDomainDto {
    Connections,
    Access,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionsRecoveryDto {
    None,
    Reobserve,
    Reconcile,
    Unlock,
    Reopen,
    NewReview,
}

/// Owner-produced safe failure metadata. Display action strings are not commands or authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsFailureDto {
    pub domain: ConnectionsFailureDomainDto,
    pub category: super::AgentFailureCategory,
    pub reason: String,
    pub incident_id: String,
    pub correlation_id: String,
    pub reload_required: bool,
    pub seal_session: bool,
    pub recovery: ConnectionsRecoveryDto,
    pub safe_actions: Vec<String>,
}

impl ConnectionsFailureDto {
    fn validate(&self) -> Result<(), &'static str> {
        if !valid_source_text(&self.reason, 256)
            || !valid_source_text(&self.incident_id, 128)
            || !valid_source_text(&self.correlation_id, 128)
            || !valid_safe_action_names(&self.safe_actions)
        {
            return Err("connections.failure");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySummaryDto {
    pub gateway_ref: super::GatewayRefDto,
    pub revision: u64,
    pub display_name: String,
    pub state: GatewayStateDto,
    pub allowed_actions: Vec<GatewayActionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<ConnectionsFailureDto>,
    pub remote_revocation_pending: bool,
}

impl GatewaySummaryDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_revision(self.revision)
            || !valid_source_text(&self.display_name, 256)
            || !unique_values(&self.allowed_actions)
        {
            return Err("connections.gateway");
        }
        if let Some(failure) = &self.failure {
            failure.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsOverviewDto {
    pub revision: u64,
    pub gateways: Vec<GatewaySummaryDto>,
    pub integrations: Vec<IntegrationSummaryDto>,
    pub sources: Vec<SourceSummaryDto>,
}

impl ConnectionsOverviewDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_revision(self.revision) {
            return Err("connections.overview.revision");
        }
        unique_by(&self.gateways, |value| value.gateway_ref.get())?;
        unique_by(&self.integrations, |value| value.integration_ref.get())?;
        unique_by(&self.sources, |value| value.source_ref.get())?;
        for value in &self.gateways {
            value.validate()?;
        }
        for value in &self.integrations {
            value.validate()?;
        }
        for value in &self.sources {
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStateDto {
    Starting,
    AwaitingLocalConfirmation,
    AwaitingGatewayApproval,
    Verifying,
    Committing,
    Connected,
    Rejected,
    Expired,
    Cancelled,
    RepairRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingActionDto {
    Confirm,
    Cancel,
    Reobserve,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairingSnapshotDto {
    pub operation_ref: super::OperationRefDto,
    pub revision: u64,
    pub state: PairingStateDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewaySummaryDto>,
    pub allowed_actions: Vec<PairingActionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<ConnectionsFailureDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_observation_after_ms: Option<u64>,
}

impl PairingSnapshotDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_revision(self.revision)
            || self.display_code.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 32 || !value.bytes().all(|byte| byte.is_ascii())
            })
            || self.expires_at.as_deref().is_some_and(|value| !valid_utc_timestamp(value))
            || self.next_observation_after_ms.is_some_and(|value| value > 60_000)
            || !unique_values(&self.allowed_actions)
        {
            return Err("connections.pairing");
        }
        if let Some(gateway) = &self.gateway {
            gateway.validate()?;
        }
        if let Some(failure) = &self.failure {
            failure.validate()?;
        }
        if self.state == PairingStateDto::Connected && self.gateway.is_none() {
            return Err("connections.pairing.state");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationSummaryDto {
    pub integration_ref: super::IntegrationRefDto,
    pub revision: u64,
    pub display_name: String,
    pub category: IntegrationCategoryDto,
    pub state: IntegrationStateDto,
    pub capabilities: Vec<IntegrationCapabilityDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSummaryDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationCategoryDto {
    Calendar,
    Contacts,
    Attention,
    Health,
    Mail,
    Work,
    Home,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStateDto {
    Available,
    Unavailable,
    Connected,
    Connecting,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationCapabilityDto {
    PrepareReview,
    Configure,
    Disconnect,
}

impl IntegrationSummaryDto {
    fn validate(&self) -> Result<(), &'static str> {
        if !valid_revision(self.revision)
            || !valid_source_text(&self.display_name, 256)
            || !unique_values(&self.capabilities)
        {
            return Err("connections.integration");
        }
        if let Some(source) = &self.source {
            source.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionOperationStateDto {
    Pending,
    Running,
    AwaitingUser,
    Completed,
    Failed,
    Cancelled,
    RepairRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionOperationActionDto {
    Cancel,
    Reobserve,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionOperationSnapshotDto {
    pub operation_ref: super::OperationRefDto,
    pub revision: u64,
    pub state: ConnectionOperationStateDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_action: Option<LaunchActionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSummaryDto>,
    pub allowed_actions: Vec<ConnectionOperationActionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<ConnectionsFailureDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_observation_after_ms: Option<u64>,
}

impl ConnectionOperationSnapshotDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_revision(self.revision)
            || self.display_code.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 32 || !value.bytes().all(|byte| byte.is_ascii())
            })
            || self.next_observation_after_ms.is_some_and(|value| value > 60_000)
            || !unique_values(&self.allowed_actions)
        {
            return Err("connections.operation");
        }
        if let Some(launch_action) = &self.launch_action {
            launch_action.validate()?;
        }
        if let Some(source) = &self.source {
            source.validate()?;
        }
        if let Some(failure) = &self.failure {
            failure.validate()?;
        }
        if matches!(self.state, ConnectionOperationStateDto::Failed | ConnectionOperationStateDto::RepairRequired)
            != self.failure.is_some()
            || (self.state == ConnectionOperationStateDto::Completed && self.source.is_none())
            || (self.state == ConnectionOperationStateDto::Cancelled && self.failure.is_some())
        {
            return Err("connections.operation.state");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAvailabilityDto {
    Available,
    Unavailable,
    PermissionRequired,
    IdentityChanged,
    Disconnected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserveStateDto {
    Disabled,
    Enabled,
    Paused,
    ReviewRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceActionDto {
    Configure,
    Disconnect,
    PrepareObserveReview,
    PauseObserve,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedResourceDto {
    pub resource_ref: super::ResourceRefDto,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSummaryDto {
    pub source_ref: super::ConnectionsSourceRefDto,
    pub revision: u64,
    pub display_labels: Vec<String>,
    pub availability: SourceAvailabilityDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_at: Option<String>,
    pub selected_resources: Vec<SelectedResourceDto>,
    pub observe_state: ObserveStateDto,
    pub allowed_actions: Vec<SourceActionDto>,
}

impl SourceSummaryDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_revision(self.revision)
            || self.display_labels.iter().any(|label| !valid_source_text(label, 256))
            || self.last_observed_at.as_deref().is_some_and(|value| !valid_utc_timestamp(value))
            || self.selected_resources.len() > 4096
            || !unique_values(&self.allowed_actions)
        {
            return Err("connections.source");
        }
        unique_by(&self.selected_resources, |value| value.resource_ref.get())?;
        if self.selected_resources.iter().any(|resource| !valid_source_text(&resource.label, 256)) {
            return Err("connections.source.resources");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PermittedResourceChoiceDto {
    pub resource_ref: super::ResourceRefDto,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDataCategoryDto {
    Personal,
    HighlySensitive,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessingDisclosureDto {
    pub current: RequestedProcessingDto,
    pub requested: RequestedProcessingDto,
    pub categories: Vec<ReviewDataCategoryDto>,
    pub scope_labels: Vec<String>,
}

impl ProcessingDisclosureDto {
    fn validate(&self) -> Result<(), &'static str> {
        if self.categories.len() > 2
            || self.categories.iter().collect::<HashSet<_>>().len() != self.categories.len()
            || self.scope_labels.iter().any(|label| !valid_source_text(label, 256))
        {
            return Err("connections.review.processing_disclosure");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReviewDto {
    pub review_ref: super::ReviewRefDto,
    pub source_ref: super::ConnectionsSourceRefDto,
    pub source_revision: u64,
    pub labels: Vec<String>,
    pub permitted_choices: Vec<PermittedResourceChoiceDto>,
    pub processing_disclosure: ProcessingDisclosureDto,
    pub expires_at: String,
    pub allowed_actions: Vec<SourceReviewActionDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceReviewActionDto {
    Configure,
}

impl SourceReviewDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        self.review_ref.validate()?;
        if !valid_revision(self.source_revision)
            || self.labels.iter().any(|label| !valid_source_text(label, 256))
            || self.permitted_choices.len() > 4096
            || self.permitted_choices.iter().any(|choice| !valid_source_text(&choice.label, 256))
            || !valid_utc_timestamp(&self.expires_at)
            || !unique_values(&self.allowed_actions)
        {
            return Err("connections.source_review");
        }
        unique_by(&self.permitted_choices, |choice| choice.resource_ref.get())?;
        self.processing_disclosure.validate()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationSetupKindDto {
    BrowserAuthorization,
    DeviceCode,
    GatewayManagedSecret,
    NativePermission,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum IntegrationTargetDto {
    #[serde(rename = "device")]
    Device { device_id: String },
    #[serde(rename = "gateway")]
    Gateway {
        gateway_ref: super::GatewayRefDto,
        gateway_revision: u64,
    },
}

impl IntegrationTargetDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Device { device_id } if valid_source_text(device_id, 256) => Ok(()),
            Self::Gateway {
                gateway_revision, ..
            } if valid_revision(*gateway_revision) => Ok(()),
            Self::Device { .. } => Err("connections.integration_review.target.device"),
            Self::Gateway { .. } => Err("connections.integration_review.target.gateway"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationReviewDto {
    pub review_ref: super::ReviewRefDto,
    pub integration_ref: super::IntegrationRefDto,
    pub catalog_revision: u64,
    pub target: IntegrationTargetDto,
    pub display_name: String,
    pub setup_kind: IntegrationSetupKindDto,
    pub expires_at: String,
    pub allowed_actions: Vec<IntegrationReviewActionDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationReviewActionDto {
    Start,
}

impl IntegrationReviewDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        self.review_ref.validate()?;
        if !valid_revision(self.catalog_revision)
            || !valid_source_text(&self.display_name, 256)
            || !valid_utc_timestamp(&self.expires_at)
            || !unique_values(&self.allowed_actions)
        {
            return Err("connections.integration_review");
        }
        self.target.validate()?;
        if matches!(self.setup_kind, IntegrationSetupKindDto::NativePermission)
            != matches!(&self.target, IntegrationTargetDto::Device { .. })
        {
            return Err("connections.integration_review.target_kind");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveReviewDto {
    pub review_ref: super::ReviewRefDto,
    pub source_ref: super::ConnectionsSourceRefDto,
    pub source_revision: u64,
    pub display_members: Vec<String>,
    pub processing_disclosure: ProcessingDisclosureDto,
    pub expires_at: String,
    pub allowed_actions: Vec<ObserveReviewActionDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserveReviewActionDto {
    Allow,
    Decline,
}

impl ObserveReviewDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        self.review_ref.validate()?;
        if !valid_revision(self.source_revision)
            || self.display_members.len() > 8
            || self.display_members.iter().any(|member| !valid_source_text(member, 128))
            || !valid_utc_timestamp(&self.expires_at)
            || !unique_values(&self.allowed_actions)
        {
            return Err("connections.observe_review");
        }
        self.processing_disclosure.validate()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchPurposeDto {
    ManageGateway,
    AuthorizeIntegration,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchActionDto {
    pub action_ref: super::LaunchActionRefDto,
    pub purpose: LaunchPurposeDto,
    pub validated_url: String,
    pub expires_at: String,
}

impl LaunchActionDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_source_text(&self.validated_url, 2048)
            || !valid_launch_url(&self.validated_url, self.purpose)
            || !valid_utc_timestamp(&self.expires_at)
        {
            return Err("connections.launch");
        }
        Ok(())
    }
}

fn valid_revision(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}

fn valid_utc_timestamp(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|timestamp| timestamp.offset().local_minus_utc() == 0)
}

fn valid_safe_action_names(values: &[String]) -> bool {
    values.iter().all(|value| valid_source_text(value, 128))
        && values.iter().collect::<HashSet<_>>().len() == values.len()
}

fn unique_values<T: Eq + std::hash::Hash>(values: &[T]) -> bool {
    values.iter().collect::<HashSet<_>>().len() == values.len()
}

fn valid_nonzero_port(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u16>().is_ok_and(|port| port != 0)
}

fn valid_launch_url(value: &str, purpose: LaunchPurposeDto) -> bool {
    if value.bytes().any(|byte| matches!(byte, b'?' | b'#' | b'@')) {
        return false;
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    if scheme != "http" {
        return false;
    }
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let Some((host, port)) = authority.split_once(':') else {
        return false;
    };
    if !["127.0.0.1", "localhost"].contains(&host)
        || authority.matches(':').count() != 1
        || !valid_nonzero_port(port)
    {
        return false;
    }
    match purpose {
        LaunchPurposeDto::ManageGateway => path == "manage",
        LaunchPurposeDto::AuthorizeIntegration => {
            let Some(operation_text) = path.strip_prefix("manage/setup/") else {
                return false;
            };
            let Ok(operation_id) = uuid::Uuid::parse_str(operation_text) else {
                return false;
            };
            !operation_id.is_nil() && operation_id.to_string() == operation_text
        }
    }
}

fn unique_by<T, K: Eq + std::hash::Hash>(values: &[T], key: impl Fn(&T) -> K) -> Result<(), &'static str> {
    if values.iter().map(key).collect::<HashSet<_>>().len() == values.len() {
        Ok(())
    } else {
        Err("connections.references")
    }
}
