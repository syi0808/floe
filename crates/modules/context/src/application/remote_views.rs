//! The remote source views a Person can be reading, and what a read of one has
//! to satisfy.
//!
//! Which views exist, which connector may serve each of them, how a view's
//! resource handle is named, and what a query and an answer must look like are
//! Context's: they decide what an authorized projection of a remote source is.
//! Access decides whether the Person granted it; the host only carries the call.

use serde::Deserialize;
use serde_json::Value;

use floe_agent_contract::AgentFailure;
use floe_context_contract::{
    CommunicationView, GrantDataCategory, LogisticsView, MAX_COMMUNICATION_BYTES, MAX_COMMUNICATION_ITEMS,
    MAX_PORTFOLIO_VIEW_BYTES, WorkContextView, validate_communication_view,
    validate_logistics_view, validate_work_context_view,
};
use floe_kernel::AGENT_VERSION;

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

/// Whether this connector may serve this view.
///
/// A mail view only ever comes from a mail connector; nothing else is a
/// communication source, however the grant is worded.
pub fn remote_view_connector_admissible(view_id: &str, connector: &str) -> bool {
    match view_id {
        MAIL_VIEW => matches!(connector, "gmail" | "microsoft.mail"),
        WORK_VIEW | LOGISTICS_VIEW => true,
        _ => false,
    }
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
