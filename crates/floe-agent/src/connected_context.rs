use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::DataClass;

pub const CONNECTED_CONTEXT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAuthority {
    Observe,
    Act,
    Interact,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutionLocation {
    Device { device_id: String },
    Server,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    Mirror,
    IndexOnDemand,
    IdentityReference,
    DerivedOnly,
    Ephemeral,
    ShortLivedCache,
    FloeCanonical,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub data_class: DataClass,
    pub retention: RetentionClass,
    pub freshness_ttl_ms: u64,
    pub max_items: usize,
    pub max_bytes: usize,
    pub provenance_required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCapabilityDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub authority: CapabilityAuthority,
    #[serde(default)]
    pub required_scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_view_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub provider: String,
    pub execution: ExecutionLocation,
    pub capabilities: Vec<ConnectorCapabilityDescriptor>,
    pub views: Vec<ViewDescriptor>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Ready,
    Degraded,
    Disconnected,
    Revoked,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFailureKind {
    CredentialExpired,
    PermissionDenied,
    PartialFetch,
    RateLimited,
    Stale,
    NoData,
    UnsupportedEntitlement,
    UnsupportedRegion,
    SourceDisagreement,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFailure {
    pub kind: SourceFailureKind,
    pub occurred_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorConnectionSnapshot {
    pub schema_version: u32,
    pub connector_id: String,
    pub state: ConnectionState,
    pub granted_scopes: Vec<String>,
    pub observed_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<SourceFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ViewSnapshot {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub item_count: usize,
    pub byte_count: usize,
    pub provenance_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorConformanceFixture {
    pub descriptor: ConnectorDescriptor,
    pub connection: ConnectorConnectionSnapshot,
    pub views: Vec<ViewSnapshot>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SituationTrigger {
    ExplicitForegroundRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SituationDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub trigger: SituationTrigger,
    pub required_view_ids: Vec<String>,
    #[serde(default)]
    pub optional_view_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceCode {
    UnsupportedVersion,
    InvalidIdentifier,
    DuplicateIdentifier,
    InvalidLimit,
    InvalidAuthority,
    CredentialExposure,
    UnknownView,
    ConnectorMismatch,
    InvalidLifecycle,
    InvalidTimestamp,
    MissingScope,
    MissingProvenance,
    ViewLimitExceeded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceViolation {
    pub code: ConformanceCode,
    pub subject: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIssue {
    pub connector_id: String,
    pub state: ConnectionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<SourceFailureKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SituationConformanceReport {
    pub situation_id: String,
    pub situation_conforms: bool,
    pub available_views: BTreeMap<String, Vec<String>>,
    pub missing_required_views: Vec<String>,
    pub missing_optional_views: Vec<String>,
    pub source_issues: Vec<SourceIssue>,
    pub violations: Vec<ConformanceViolation>,
}

impl SituationConformanceReport {
    pub fn can_run(&self) -> bool {
        self.situation_conforms && self.missing_required_views.is_empty()
    }
}

pub fn validate_connector_fixture(
    fixture: &ConnectorConformanceFixture,
    now_unix_ms: u64,
) -> Vec<ConformanceViolation> {
    let mut violations = Vec::new();
    let descriptor = &fixture.descriptor;
    let connection = &fixture.connection;
    validate_version(descriptor.schema_version, &descriptor.id, &mut violations);
    validate_identifier(&descriptor.id, "connector", &mut violations);
    validate_identifier(&descriptor.version, &descriptor.id, &mut violations);
    validate_identifier(&descriptor.provider, &descriptor.id, &mut violations);
    if let ExecutionLocation::Device { device_id } = &descriptor.execution {
        validate_identifier(device_id, &descriptor.id, &mut violations);
    }

    let mut view_ids = BTreeSet::new();
    for view in &descriptor.views {
        validate_version(view.schema_version, &view.id, &mut violations);
        validate_identifier(&view.id, "view", &mut violations);
        validate_identifier(&view.version, &view.id, &mut violations);
        if !view_ids.insert(view.id.as_str()) {
            push(
                &mut violations,
                ConformanceCode::DuplicateIdentifier,
                &view.id,
            );
        }
        if view.freshness_ttl_ms == 0 || view.max_items == 0 || view.max_bytes == 0 {
            push(&mut violations, ConformanceCode::InvalidLimit, &view.id);
        }
        if view.data_class == DataClass::Credential {
            push(
                &mut violations,
                ConformanceCode::CredentialExposure,
                &view.id,
            );
        }
    }

    let mut capability_ids = BTreeSet::new();
    for capability in &descriptor.capabilities {
        validate_version(capability.schema_version, &capability.id, &mut violations);
        validate_identifier(&capability.id, "capability", &mut violations);
        validate_identifier(&capability.version, &capability.id, &mut violations);
        if !capability_ids.insert(capability.id.as_str()) {
            push(
                &mut violations,
                ConformanceCode::DuplicateIdentifier,
                &capability.id,
            );
        }
        let mut scopes = BTreeSet::new();
        for scope in &capability.required_scopes {
            validate_identifier(scope, &capability.id, &mut violations);
            if !scopes.insert(scope.as_str()) {
                push(
                    &mut violations,
                    ConformanceCode::DuplicateIdentifier,
                    &capability.id,
                );
            }
        }
        match (capability.authority, capability.output_view_id.as_deref()) {
            (CapabilityAuthority::Observe, Some(view_id)) if view_ids.contains(view_id) => {}
            (CapabilityAuthority::Observe, Some(view_id)) => {
                push(&mut violations, ConformanceCode::UnknownView, view_id)
            }
            (CapabilityAuthority::Observe, None)
            | (CapabilityAuthority::Act | CapabilityAuthority::Interact, Some(_)) => push(
                &mut violations,
                ConformanceCode::InvalidAuthority,
                &capability.id,
            ),
            (CapabilityAuthority::Act | CapabilityAuthority::Interact, None) => {}
        }
    }

    validate_version(
        connection.schema_version,
        &connection.connector_id,
        &mut violations,
    );
    if connection.connector_id != descriptor.id {
        push(
            &mut violations,
            ConformanceCode::ConnectorMismatch,
            &connection.connector_id,
        );
    }
    if connection.observed_at_unix_ms > now_unix_ms
        || connection
            .last_success_at_unix_ms
            .is_some_and(|timestamp| timestamp > connection.observed_at_unix_ms)
        || connection
            .last_failure
            .as_ref()
            .is_some_and(|failure| failure.occurred_at_unix_ms > connection.observed_at_unix_ms)
    {
        push(
            &mut violations,
            ConformanceCode::InvalidTimestamp,
            &connection.connector_id,
        );
    }
    match connection.state {
        ConnectionState::Ready
            if connection.last_success_at_unix_ms.is_none()
                || connection.last_failure.is_some() =>
        {
            push(
                &mut violations,
                ConformanceCode::InvalidLifecycle,
                &connection.connector_id,
            )
        }
        ConnectionState::Degraded if connection.last_failure.is_none() => push(
            &mut violations,
            ConformanceCode::InvalidLifecycle,
            &connection.connector_id,
        ),
        ConnectionState::Revoked | ConnectionState::Unsupported
            if connection.last_failure.is_none() =>
        {
            push(
                &mut violations,
                ConformanceCode::InvalidLifecycle,
                &connection.connector_id,
            )
        }
        _ => {}
    }

    let mut seen_granted_scopes = BTreeSet::new();
    for scope in &connection.granted_scopes {
        validate_identifier(scope, &connection.connector_id, &mut violations);
        if !seen_granted_scopes.insert(scope.as_str()) {
            push(
                &mut violations,
                ConformanceCode::DuplicateIdentifier,
                &connection.connector_id,
            );
        }
    }
    let granted_scopes: BTreeSet<_> = connection
        .granted_scopes
        .iter()
        .map(String::as_str)
        .collect();
    let observable_views: BTreeSet<_> = descriptor
        .capabilities
        .iter()
        .filter(|capability| capability.authority == CapabilityAuthority::Observe)
        .filter(|capability| {
            capability
                .required_scopes
                .iter()
                .all(|scope| granted_scopes.contains(scope.as_str()))
        })
        .filter_map(|capability| capability.output_view_id.as_deref())
        .collect();
    for capability in &descriptor.capabilities {
        if capability.authority == CapabilityAuthority::Observe
            && capability
                .required_scopes
                .iter()
                .any(|scope| !granted_scopes.contains(scope.as_str()))
            && fixture
                .views
                .iter()
                .any(|view| capability.output_view_id.as_deref() == Some(view.view_id.as_str()))
        {
            push(
                &mut violations,
                ConformanceCode::MissingScope,
                &capability.id,
            );
        }
    }

    for snapshot in &fixture.views {
        validate_version(snapshot.schema_version, &snapshot.view_id, &mut violations);
        let Some(view) = descriptor
            .views
            .iter()
            .find(|view| view.id == snapshot.view_id)
        else {
            push(
                &mut violations,
                ConformanceCode::UnknownView,
                &snapshot.view_id,
            );
            continue;
        };
        if snapshot.source_handle.trim().is_empty() {
            push(
                &mut violations,
                ConformanceCode::InvalidIdentifier,
                &snapshot.view_id,
            );
        }
        if !observable_views.contains(snapshot.view_id.as_str()) {
            push(
                &mut violations,
                ConformanceCode::MissingScope,
                &snapshot.view_id,
            );
        }
        if snapshot.observed_at_unix_ms > connection.observed_at_unix_ms
            || connection
                .last_success_at_unix_ms
                .is_none_or(|last_success| snapshot.observed_at_unix_ms > last_success)
            || snapshot.expires_at_unix_ms <= snapshot.observed_at_unix_ms
            || snapshot.expires_at_unix_ms - snapshot.observed_at_unix_ms > view.freshness_ttl_ms
        {
            push(
                &mut violations,
                ConformanceCode::InvalidTimestamp,
                &snapshot.view_id,
            );
        }
        if snapshot.item_count > view.max_items || snapshot.byte_count > view.max_bytes {
            push(
                &mut violations,
                ConformanceCode::ViewLimitExceeded,
                &snapshot.view_id,
            );
        }
        if view.provenance_required
            && snapshot.item_count > 0
            && snapshot.provenance_count < snapshot.item_count
        {
            push(
                &mut violations,
                ConformanceCode::MissingProvenance,
                &snapshot.view_id,
            );
        }
    }
    violations
}

pub fn evaluate_situation(
    situation: &SituationDescriptor,
    fixtures: &[ConnectorConformanceFixture],
    now_unix_ms: u64,
) -> SituationConformanceReport {
    let mut violations = Vec::new();
    validate_version(situation.schema_version, &situation.id, &mut violations);
    validate_identifier(&situation.id, "situation", &mut violations);
    validate_identifier(&situation.version, &situation.id, &mut violations);
    if situation.required_view_ids.is_empty() {
        push(
            &mut violations,
            ConformanceCode::InvalidLimit,
            &situation.id,
        );
    }

    let mut requested = BTreeSet::new();
    for view_id in situation
        .required_view_ids
        .iter()
        .chain(&situation.optional_view_ids)
    {
        validate_identifier(view_id, &situation.id, &mut violations);
        if !requested.insert(view_id.as_str()) {
            push(
                &mut violations,
                ConformanceCode::DuplicateIdentifier,
                view_id,
            );
        }
    }
    let situation_conforms = violations.is_empty();

    let mut available_views: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut source_issues = Vec::new();
    for fixture in fixtures {
        let fixture_violations = validate_connector_fixture(fixture, now_unix_ms);
        let fixture_conforms = fixture_violations.is_empty();
        violations.extend(fixture_violations);
        if fixture.connection.state != ConnectionState::Ready
            && fixture.connection.state != ConnectionState::Degraded
        {
            source_issues.push(SourceIssue {
                connector_id: fixture.descriptor.id.clone(),
                state: fixture.connection.state,
                failure: fixture
                    .connection
                    .last_failure
                    .as_ref()
                    .map(|failure| failure.kind),
            });
            continue;
        }
        if fixture.connection.state == ConnectionState::Degraded {
            source_issues.push(SourceIssue {
                connector_id: fixture.descriptor.id.clone(),
                state: fixture.connection.state,
                failure: fixture
                    .connection
                    .last_failure
                    .as_ref()
                    .map(|failure| failure.kind),
            });
        }
        if !fixture_conforms {
            continue;
        }
        for snapshot in &fixture.views {
            if snapshot.expires_at_unix_ms > now_unix_ms
                && requested.contains(snapshot.view_id.as_str())
            {
                available_views
                    .entry(snapshot.view_id.clone())
                    .or_default()
                    .push(snapshot.source_handle.clone());
            }
        }
    }
    for sources in available_views.values_mut() {
        sources.sort();
        sources.dedup();
    }

    let missing_required_views = situation
        .required_view_ids
        .iter()
        .filter(|view_id| !available_views.contains_key(view_id.as_str()))
        .cloned()
        .collect();
    let missing_optional_views = situation
        .optional_view_ids
        .iter()
        .filter(|view_id| !available_views.contains_key(view_id.as_str()))
        .cloned()
        .collect();
    SituationConformanceReport {
        situation_id: situation.id.clone(),
        situation_conforms,
        available_views,
        missing_required_views,
        missing_optional_views,
        source_issues,
        violations,
    }
}

fn validate_version(version: u32, subject: &str, violations: &mut Vec<ConformanceViolation>) {
    if version != CONNECTED_CONTEXT_VERSION {
        push(violations, ConformanceCode::UnsupportedVersion, subject);
    }
}

fn validate_identifier(value: &str, subject: &str, violations: &mut Vec<ConformanceViolation>) {
    if value.trim().is_empty() || value.len() > 128 {
        push(violations, ConformanceCode::InvalidIdentifier, subject);
    }
}

fn push(violations: &mut Vec<ConformanceViolation>, code: ConformanceCode, subject: &str) {
    violations.push(ConformanceViolation {
        code,
        subject: subject.to_owned(),
    });
}
