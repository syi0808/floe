use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ConnectionState, ConnectorSnapshot, ViewSnapshot, validate_connector_snapshot};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTransferClass {
    DeviceOnly,
    OpaqueRelay,
    EncryptedDerivedSync,
    #[default]
    DeclaredServerProcessing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    Mobile,
    Desktop,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePresence {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDeviceState {
    pub device_id: String,
    pub class: DeviceClass,
    pub online: bool,
    pub presence: DevicePresence,
    pub presence_expires_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSourcePolicy {
    pub connector_id: String,
    pub transfer: ContextTransferClass,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceScope {
    #[default]
    Any,
    Location {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        invoking_mobile_device_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        designated_carried_device_id: Option<String>,
    },
    Attention {
        interaction_device_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active_desktop_device_id: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogicalViewRoutingPolicy {
    pub logical_source_id: String,
    #[serde(default)]
    pub device_scope: DeviceScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<u64>,
    #[serde(default)]
    pub allowed_transfers: Vec<ContextTransferClass>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRoutingRuntime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_device_id: Option<String>,
    #[serde(default)]
    pub devices: Vec<RuntimeDeviceState>,
    #[serde(default)]
    pub sources: Vec<RuntimeSourcePolicy>,
    #[serde(default)]
    pub logical_views: Vec<LogicalViewRoutingPolicy>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutedAvailability {
    #[default]
    Fresh,
    DegradedOfflineCache,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogicalViewRoute {
    pub logical_source_id: String,
    pub connector_id: String,
    pub view_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutedView {
    pub logical_source_id: String,
    pub connector_id: String,
    pub provider: String,
    pub state: ConnectionState,
    pub view: ViewSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<ContextTransferClass>,
    #[serde(default, skip_serializing_if = "is_fresh")]
    pub availability: RoutedAvailability,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDisagreement {
    pub logical_source_id: String,
    pub candidates: Vec<RoutedView>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRouteResult {
    pub selected: Vec<RoutedView>,
    pub unavailable_logical_sources: Vec<String>,
    pub deduplicated_candidates: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disagreements: Vec<RouteDisagreement>,
}

pub fn route_logical_views(
    routes: &[LogicalViewRoute],
    snapshots: &[ConnectorSnapshot],
    provider_priority: &[String],
    now_unix_ms: u64,
) -> ContextRouteResult {
    route_logical_views_inner(
        routes,
        snapshots,
        provider_priority,
        now_unix_ms,
        &ContextRoutingRuntime::default(),
        false,
    )
}

pub fn route_logical_views_with_runtime(
    routes: &[LogicalViewRoute],
    snapshots: &[ConnectorSnapshot],
    provider_priority: &[String],
    now_unix_ms: u64,
    runtime: &ContextRoutingRuntime,
) -> ContextRouteResult {
    route_logical_views_inner(
        routes,
        snapshots,
        provider_priority,
        now_unix_ms,
        runtime,
        true,
    )
}

fn route_logical_views_inner(
    routes: &[LogicalViewRoute],
    snapshots: &[ConnectorSnapshot],
    provider_priority: &[String],
    now_unix_ms: u64,
    runtime: &ContextRoutingRuntime,
    enforce_runtime_policy: bool,
) -> ContextRouteResult {
    let priorities: BTreeMap<_, _> = provider_priority
        .iter()
        .enumerate()
        .map(|(index, provider)| (provider.as_str(), index))
        .collect();
    let mut candidates: BTreeMap<&str, Vec<(RoutedView, bool, usize)>> = BTreeMap::new();
    let mut unavailable: BTreeSet<_> = routes
        .iter()
        .filter(|route| valid_identifier(&route.logical_source_id))
        .map(|route| route.logical_source_id.clone())
        .collect();
    for route in routes {
        if !valid_identifier(&route.logical_source_id)
            || !valid_identifier(&route.connector_id)
            || !valid_identifier(&route.view_id)
        {
            continue;
        }
        let Some(snapshot) = snapshots.iter().find(|snapshot| {
            snapshot.descriptor.id == route.connector_id
                && snapshot.connection.connector_id == route.connector_id
        }) else {
            continue;
        };
        if !matches!(
            snapshot.connection.state,
            ConnectionState::Ready | ConnectionState::Degraded
        ) || !validate_connector_snapshot(snapshot, now_unix_ms).is_empty()
        {
            continue;
        }
        let Some(view) = snapshot
            .views
            .iter()
            .find(|view| view.view_id == route.view_id && view.expires_at_unix_ms > now_unix_ms)
        else {
            continue;
        };
        let policy = runtime
            .logical_views
            .iter()
            .find(|policy| policy.logical_source_id == route.logical_source_id);
        if policy
            .and_then(|policy| policy.max_age_ms)
            .is_some_and(|max_age| now_unix_ms.saturating_sub(view.observed_at_unix_ms) > max_age)
        {
            continue;
        }
        let transfer = runtime
            .sources
            .iter()
            .find(|source| source.connector_id == route.connector_id)
            .map(|source| source.transfer);
        let producer_device_id = match &snapshot.descriptor.execution {
            crate::ExecutionLocation::Device { device_id } => Some(device_id.clone()),
            crate::ExecutionLocation::Server => None,
        };
        let local_device_source = producer_device_id.as_deref()
            == runtime.consumer_device_id.as_deref()
            && producer_device_id.is_some();
        if enforce_runtime_policy
            && producer_device_id.is_some()
            && !local_device_source
            && (policy.is_none()
                || transfer.is_none()
                || policy.is_some_and(|policy| {
                    policy.allowed_transfers.is_empty()
                        || transfer.is_none_or(|transfer| {
                            !policy.allowed_transfers.contains(&transfer)
                                || transfer == ContextTransferClass::DeviceOnly
                        })
                }))
        {
            continue;
        }
        let Some((availability, device_rank)) = route_device(
            policy.map(|policy| &policy.device_scope),
            producer_device_id.as_deref(),
            transfer,
            runtime,
            now_unix_ms,
            enforce_runtime_policy,
        ) else {
            continue;
        };
        let reports_disagreement = snapshot
            .connection
            .last_failure
            .as_ref()
            .is_some_and(|failure| failure.kind == crate::SourceFailureKind::SourceDisagreement);
        candidates
            .entry(&route.logical_source_id)
            .or_default()
            .push((
                RoutedView {
                    logical_source_id: route.logical_source_id.clone(),
                    connector_id: route.connector_id.clone(),
                    provider: snapshot.descriptor.provider.clone(),
                    state: snapshot.connection.state,
                    view: view.clone(),
                    producer_device_id,
                    transfer,
                    availability,
                },
                reports_disagreement,
                device_rank,
            ));
    }

    let mut selected = Vec::new();
    let mut disagreements = Vec::new();
    let mut seen_sources = BTreeSet::new();
    let mut deduplicated_candidates = 0;
    for (logical_source, mut options) in candidates {
        options.sort_by(|left, right| {
            left.2
                .cmp(&right.2)
                .then_with(|| {
                    availability_rank(left.0.availability)
                        .cmp(&availability_rank(right.0.availability))
                })
                .then_with(|| route_rank(left.0.state).cmp(&route_rank(right.0.state)))
                .then_with(|| {
                    right
                        .0
                        .view
                        .observed_at_unix_ms
                        .cmp(&left.0.view.observed_at_unix_ms)
                })
                .then_with(|| {
                    priorities
                        .get(left.0.provider.as_str())
                        .copied()
                        .unwrap_or(usize::MAX)
                        .cmp(
                            &priorities
                                .get(right.0.provider.as_str())
                                .copied()
                                .unwrap_or(usize::MAX),
                        )
                })
                .then_with(|| left.0.connector_id.cmp(&right.0.connector_id))
        });
        let option_count = options.len();
        let disagreement_reported = options.iter().any(|candidate| candidate.1);
        let mut distinct = Vec::new();
        let mut distinct_handles = BTreeSet::new();
        for (candidate, _, _) in &options {
            if distinct_handles.insert(candidate.view.source_handle.as_str()) {
                distinct.push(candidate.clone());
            }
        }
        if disagreement_reported {
            disagreements.push(RouteDisagreement {
                logical_source_id: logical_source.into(),
                candidates: distinct,
            });
        }
        if let Some((chosen, _, _)) = options
            .into_iter()
            .find(|candidate| seen_sources.insert(candidate.0.view.source_handle.clone()))
        {
            deduplicated_candidates += option_count.saturating_sub(1);
            unavailable.remove(logical_source);
            selected.push(chosen);
        } else {
            deduplicated_candidates += option_count;
        }
    }
    ContextRouteResult {
        selected,
        unavailable_logical_sources: unavailable.into_iter().collect(),
        deduplicated_candidates,
        disagreements,
    }
}

fn route_device(
    scope: Option<&DeviceScope>,
    producer_device_id: Option<&str>,
    transfer: Option<ContextTransferClass>,
    runtime: &ContextRoutingRuntime,
    now_unix_ms: u64,
    enforce_runtime_policy: bool,
) -> Option<(RoutedAvailability, usize)> {
    let scope = scope.unwrap_or(&DeviceScope::Any);
    let Some(device_id) = producer_device_id else {
        return matches!(scope, DeviceScope::Any).then_some((RoutedAvailability::Fresh, 0));
    };
    let device = runtime
        .devices
        .iter()
        .find(|device| device.device_id == device_id);
    if matches!(scope, DeviceScope::Any) && device.is_none() && !enforce_runtime_policy {
        return Some((RoutedAvailability::Fresh, 0));
    }
    let device = device?;
    let present = device.presence == DevicePresence::Present
        && device.presence_expires_at_unix_ms > now_unix_ms;
    let rank = match scope {
        DeviceScope::Any => 0,
        DeviceScope::Location {
            invoking_mobile_device_id,
            designated_carried_device_id,
        } => {
            if device.class != DeviceClass::Mobile || !present {
                return None;
            }
            if invoking_mobile_device_id.as_deref() == Some(device_id) {
                0
            } else if designated_carried_device_id.as_deref() == Some(device_id) {
                1
            } else {
                return None;
            }
        }
        DeviceScope::Attention {
            interaction_device_id,
            active_desktop_device_id,
        } => {
            if interaction_device_id == device_id {
                0
            } else if active_desktop_device_id.as_deref() == Some(device_id)
                && device.class == DeviceClass::Desktop
                && present
            {
                1
            } else {
                return None;
            }
        }
    };
    if device.online {
        Some((RoutedAvailability::Fresh, rank))
    } else if transfer.is_some_and(|transfer| transfer != ContextTransferClass::DeviceOnly) {
        Some((RoutedAvailability::DegradedOfflineCache, rank))
    } else {
        None
    }
}

fn availability_rank(availability: RoutedAvailability) -> u8 {
    match availability {
        RoutedAvailability::Fresh => 0,
        RoutedAvailability::DegradedOfflineCache => 1,
    }
}

fn is_fresh(availability: &RoutedAvailability) -> bool {
    *availability == RoutedAvailability::Fresh
}

fn route_rank(state: ConnectionState) -> u8 {
    match state {
        ConnectionState::Ready => 0,
        ConnectionState::Degraded => 1,
        _ => 2,
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}
