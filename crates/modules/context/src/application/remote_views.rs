//! The remote source views a Person can be reading, and what a read of one has
//! to satisfy.
//!
//! Which views exist, how a view's
//! resource handle is named, and what a query and an answer must look like are
//! Context's: they decide what an authorized projection of a remote source is.
//! Access decides whether the Person granted it; the host only carries the call.

use serde::Deserialize;
use serde_json::Value;

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_context_contract::{
    CommunicationView, GrantDataCategory, LogisticsView, MAX_COMMUNICATION_BYTES,
    MAX_COMMUNICATION_ITEMS, MAX_PORTFOLIO_VIEW_BYTES, WorkContextView,
    validate_communication_view, validate_logistics_view, validate_work_context_view,
};
use sha2::{Digest, Sha256};

pub const MAIL_VIEW: &str = "mail.communication";
pub const WORK_VIEW: &str = "work.context";
pub const LOGISTICS_VIEW: &str = "life.logistics";

/// Whether this is a remote view a Person can hold a grant for at all.
pub fn is_remote_view(view_id: &str) -> bool {
    matches!(view_id, MAIL_VIEW | WORK_VIEW | LOGISTICS_VIEW)
}

/// The resource handle a grant must name to admit this view on this connection.
pub fn remote_view_resource(view_id: &str, connection_id: &str) -> String {
    format!("{view_id}:{connection_id}")
}

/// The view and connection one resource handle names.
///
/// A handle that does not name a remote view on the connection the read is
/// bound to is not this read's, whatever it spells.
pub fn split_remote_view_resource<'a>(
    resource: &'a str,
    connection_id: &str,
) -> Result<&'a str, AgentFailure> {
    let (view_id, named_connection) = resource.split_once(':').ok_or(AgentFailure::PolicyDenied)?;
    if named_connection != connection_id
        || !is_remote_view(view_id)
        || remote_view_resource(view_id, connection_id) != resource
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(view_id)
}

/// What a grant for this view covers.
///
/// A communication view carries the Person's own correspondence, so a grant for
/// it is a grant over content; the other views are derived projections.
pub fn remote_view_data_category(view_id: &str) -> GrantDataCategory {
    if view_id == MAIL_VIEW {
        GrantDataCategory::Content
    } else {
        GrantDataCategory::Derived
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MailQuery {
    schema_version: u32,
    query: String,
    cursor: usize,
    limit: usize,
}

/// What one query may ask this view for, and the item and byte bounds the
/// answer stands under.
pub fn validate_remote_view_query(
    view_id: &str,
    query: &Value,
) -> Result<(usize, usize), AgentFailure> {
    match view_id {
        MAIL_VIEW => {
            let query: MailQuery =
                serde_json::from_value(query.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            if query.schema_version != AGENT_VERSION
                || query.query.len() > 512
                || query.cursor > 10_000
                || !(1..=MAX_COMMUNICATION_ITEMS).contains(&query.limit)
            {
                return Err(AgentFailure::InvalidInput);
            }
            Ok((query.limit, MAX_COMMUNICATION_BYTES))
        }
        WORK_VIEW | LOGISTICS_VIEW
            if query == &serde_json::json!({"schema_version": AGENT_VERSION}) =>
        {
            Ok((1, MAX_PORTFOLIO_VIEW_BYTES))
        }
        _ => Err(AgentFailure::InvalidInput),
    }
}

/// The answer a remote view returned, re-encoded from the shape it must have,
/// with the freshness window it claims.
pub fn validate_remote_view(
    view_id: &str,
    value: Value,
    now: i64,
    max_items: usize,
    max_bytes: usize,
) -> Result<(Value, i64, i64), AgentFailure> {
    match view_id {
        MAIL_VIEW => {
            let view: CommunicationView =
                serde_json::from_value(value).map_err(|_| AgentFailure::CapabilityUnavailable)?;
            validate_communication_view(&view, now, max_items, max_bytes)?;
            let result =
                serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidModelOutput)?;
            Ok((result, view.observed_at_unix_ms, view.expires_at_unix_ms))
        }
        WORK_VIEW => {
            let view: WorkContextView =
                serde_json::from_value(value).map_err(|_| AgentFailure::CapabilityUnavailable)?;
            validate_work_context_view(&view, now)?;
            let result =
                serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidModelOutput)?;
            Ok((result, view.observed_at_unix_ms, view.expires_at_unix_ms))
        }
        LOGISTICS_VIEW => {
            let view: LogisticsView =
                serde_json::from_value(value).map_err(|_| AgentFailure::CapabilityUnavailable)?;
            validate_logistics_view(&view, now)?;
            let result =
                serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidModelOutput)?;
            Ok((result, view.observed_at_unix_ms, view.expires_at_unix_ms))
        }
        _ => Err(AgentFailure::InvalidInput),
    }
}

pub fn merge_remote_views(
    view_id: &str,
    values: Vec<Value>,
    now: i64,
    max_items: usize,
    max_bytes: usize,
) -> Result<Value, AgentFailure> {
    if values.is_empty() {
        return Err(AgentFailure::AccessReviewRequired);
    }
    if values.len() == 1 {
        return validate_remote_view(
            view_id,
            values.into_iter().next().unwrap(),
            now,
            max_items,
            max_bytes,
        )
        .map(|result| result.0);
    }
    let mut source_handles = values
        .iter()
        .filter_map(|value| value.get("source_handle").and_then(Value::as_str))
        .collect::<Vec<_>>();
    source_handles.sort_unstable();
    let digest = Sha256::digest(source_handles.join("\n").as_bytes());
    let fingerprint = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let source_handle = format!("multi:{view_id}:{fingerprint}");
    match view_id {
        MAIL_VIEW => {
            let mut views = values
                .into_iter()
                .map(serde_json::from_value::<CommunicationView>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?;
            let observed_at_unix_ms = views
                .iter()
                .map(|view| view.observed_at_unix_ms)
                .max()
                .unwrap();
            let expires_at_unix_ms = views
                .iter()
                .map(|view| view.expires_at_unix_ms)
                .min()
                .unwrap();
            let complete = views.iter().all(|view| view.coverage_complete);
            let mut items = views
                .drain(..)
                .flat_map(|view| view.items)
                .collect::<Vec<_>>();
            items.sort_by(|left, right| {
                right
                    .received_unix_ms
                    .cmp(&left.received_unix_ms)
                    .then_with(|| left.evidence_handle.cmp(&right.evidence_handle))
            });
            items.dedup_by(|left, right| left.evidence_handle == right.evidence_handle);
            let truncated = items.len() > max_items;
            items.truncate(max_items);
            let view = CommunicationView {
                schema_version: AGENT_VERSION,
                view_id: MAIL_VIEW.into(),
                source_handle,
                observed_at_unix_ms,
                expires_at_unix_ms,
                coverage_complete: complete && !truncated,
                next_cursor: None,
                items,
            };
            validate_communication_view(&view, now, max_items, max_bytes)?;
            serde_json::to_value(view).map_err(|_| AgentFailure::InvalidModelOutput)
        }
        WORK_VIEW => {
            let views = values
                .into_iter()
                .map(serde_json::from_value::<WorkContextView>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?;
            let observed_at_unix_ms = views
                .iter()
                .map(|view| view.observed_at_unix_ms)
                .max()
                .unwrap();
            let expires_at_unix_ms = views
                .iter()
                .map(|view| view.expires_at_unix_ms)
                .min()
                .unwrap();
            let complete = views.iter().all(|view| view.coverage_complete);
            let mut items = views
                .into_iter()
                .flat_map(|view| view.items)
                .collect::<Vec<_>>();
            items.sort_by(|left, right| {
                right
                    .observed_at_unix_ms
                    .cmp(&left.observed_at_unix_ms)
                    .then_with(|| left.evidence_handle.cmp(&right.evidence_handle))
            });
            items.dedup_by(|left, right| left.evidence_handle == right.evidence_handle);
            let truncated = items.len() > 64;
            items.truncate(64);
            let view = WorkContextView {
                schema_version: AGENT_VERSION,
                view_id: WORK_VIEW.into(),
                source_handle,
                observed_at_unix_ms,
                expires_at_unix_ms,
                coverage_complete: complete && !truncated,
                scope_handle: "multi:work.context".into(),
                items,
            };
            validate_work_context_view(&view, now)?;
            if serde_json::to_vec(&view)
                .map_err(|_| AgentFailure::InvalidInput)?
                .len()
                > max_bytes
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            serde_json::to_value(view).map_err(|_| AgentFailure::InvalidModelOutput)
        }
        LOGISTICS_VIEW => {
            let views = values
                .into_iter()
                .map(serde_json::from_value::<LogisticsView>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?;
            let observed_at_unix_ms = views
                .iter()
                .map(|view| view.observed_at_unix_ms)
                .max()
                .unwrap();
            let expires_at_unix_ms = views
                .iter()
                .map(|view| view.expires_at_unix_ms)
                .min()
                .unwrap();
            let complete = views.iter().all(|view| view.coverage_complete);
            let mut items = views
                .into_iter()
                .flat_map(|view| view.items)
                .collect::<Vec<_>>();
            items.sort_by(|left, right| {
                left.occurs_at_unix_ms
                    .cmp(&right.occurs_at_unix_ms)
                    .then_with(|| left.evidence_handle.cmp(&right.evidence_handle))
            });
            items.dedup_by(|left, right| left.evidence_handle == right.evidence_handle);
            let truncated = items.len() > 64;
            items.truncate(64);
            let view = LogisticsView {
                schema_version: AGENT_VERSION,
                view_id: LOGISTICS_VIEW.into(),
                source_handle,
                observed_at_unix_ms,
                expires_at_unix_ms,
                coverage_complete: complete && !truncated,
                items,
            };
            validate_logistics_view(&view, now)?;
            if serde_json::to_vec(&view)
                .map_err(|_| AgentFailure::InvalidInput)?
                .len()
                > max_bytes
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            serde_json::to_value(view).map_err(|_| AgentFailure::InvalidModelOutput)
        }
        _ => Err(AgentFailure::InvalidInput),
    }
}

/// What one remote view read owes its provenance to.
///
/// The categories, the processing restriction and the consumer policy come from
/// the grant the read ran under, not from the caller: a dependency describes the
/// authority it was produced beneath.
#[allow(clippy::too_many_arguments)]
pub fn remote_view_dependency(
    person_id: floe_agent_contract::PersonId,
    grant: &floe_access::DataAccessGrant,
    consumer_policy: floe_access::ConsumerPolicyAuthority,
    source: floe_access::GrantSourceBinding,
    resource: &str,
    consumer: floe_access::GrantConsumer,
    query_fingerprint: Vec<u8>,
    lease_invocation_id: uuid::Uuid,
    process_incarnation_id: uuid::Uuid,
    observed_at_unix_ms: i64,
    expires_at_unix_ms: i64,
) -> Result<floe_access::ContextDependency, AgentFailure> {
    let observed = chrono::DateTime::from_timestamp_millis(observed_at_unix_ms)
        .ok_or(AgentFailure::StaleContext)?;
    let expires = chrono::DateTime::from_timestamp_millis(expires_at_unix_ms)
        .ok_or(AgentFailure::StaleContext)?;
    floe_access::ContextDependency::try_new(
        person_id,
        grant.id(),
        grant.authority(),
        source,
        vec![
            floe_access::ResourceHandle::try_new(resource)
                .map_err(|_| AgentFailure::InvalidInput)?,
        ],
        grant.scope().categories().to_vec(),
        floe_access::GrantOperation::Read,
        floe_access::GrantPurpose::Assistant,
        consumer,
        grant.scope().processing().clone(),
        consumer_policy,
        uuid::Uuid::new_v4(),
        query_fingerprint,
        lease_invocation_id,
        process_incarnation_id,
        observed,
        expires,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn mail(source: &str, evidence: &str, received: i64, complete: bool) -> Value {
        serde_json::json!({
            "schema_version": AGENT_VERSION,
            "view_id": MAIL_VIEW,
            "source_handle": source,
            "observed_at_unix_ms": 1_000,
            "expires_at_unix_ms": 10_000,
            "coverage_complete": complete,
            "items": [{
                "evidence_handle": evidence,
                "thread_handle": format!("thread:{evidence}"),
                "received_unix_ms": received,
                "from": "sender",
                "to": "person",
                "subject": "subject",
                "snippet": "snippet",
                "labels": []
            }]
        })
    }

    #[test]
    fn communication_merge_is_bounded_deterministic_and_partial() {
        let merged = merge_remote_views(
            MAIL_VIEW,
            vec![
                mail("gmail:one", "gmail:message", 20, true),
                mail("microsoft:one", "microsoft:message", 30, false),
            ],
            2_000,
            8,
            MAX_COMMUNICATION_BYTES,
        )
        .unwrap();
        let view: CommunicationView = serde_json::from_value(merged).unwrap();
        assert_eq!(view.items.len(), 2);
        assert_eq!(view.items[0].evidence_handle, "microsoft:message");
        assert!(!view.coverage_complete);
        assert!(view.source_handle.starts_with("multi:mail.communication:"));
    }
}
