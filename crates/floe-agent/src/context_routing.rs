use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ConnectionState, ConnectorSnapshot, ViewSnapshot, validate_connector_snapshot};

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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRouteResult {
    pub selected: Vec<RoutedView>,
    pub unavailable_logical_sources: Vec<String>,
    pub deduplicated_candidates: usize,
}

pub fn route_logical_views(
    routes: &[LogicalViewRoute],
    snapshots: &[ConnectorSnapshot],
    provider_priority: &[String],
    now_unix_ms: u64,
) -> ContextRouteResult {
    let priorities: BTreeMap<_, _> = provider_priority
        .iter()
        .enumerate()
        .map(|(index, provider)| (provider.as_str(), index))
        .collect();
    let mut candidates: BTreeMap<&str, Vec<RoutedView>> = BTreeMap::new();
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
        candidates
            .entry(&route.logical_source_id)
            .or_default()
            .push(RoutedView {
                logical_source_id: route.logical_source_id.clone(),
                connector_id: route.connector_id.clone(),
                provider: snapshot.descriptor.provider.clone(),
                state: snapshot.connection.state,
                view: view.clone(),
            });
    }

    let mut selected = Vec::new();
    let mut seen_sources = BTreeSet::new();
    let mut deduplicated_candidates = 0;
    for (logical_source, mut options) in candidates {
        options.sort_by(|left, right| {
            route_rank(left.state)
                .cmp(&route_rank(right.state))
                .then_with(|| {
                    right
                        .view
                        .observed_at_unix_ms
                        .cmp(&left.view.observed_at_unix_ms)
                })
                .then_with(|| {
                    priorities
                        .get(left.provider.as_str())
                        .copied()
                        .unwrap_or(usize::MAX)
                        .cmp(
                            &priorities
                                .get(right.provider.as_str())
                                .copied()
                                .unwrap_or(usize::MAX),
                        )
                })
                .then_with(|| left.connector_id.cmp(&right.connector_id))
        });
        let option_count = options.len();
        if let Some(chosen) = options
            .into_iter()
            .find(|candidate| seen_sources.insert(candidate.view.source_handle.clone()))
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
    }
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
