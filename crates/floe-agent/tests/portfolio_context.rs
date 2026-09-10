use floe_agent::{
    AGENT_VERSION, AgentFailure, LogisticsItem, LogisticsItemKind, LogisticsView, WorkContextItem,
    WorkContextView, WorkItemKind, logistics_context_evidence, validate_logistics_view,
    validate_work_context_view, work_context_evidence,
};

const NOW: i64 = 1_789_000_000_000;

fn work() -> WorkContextView {
    WorkContextView {
        schema_version: AGENT_VERSION,
        view_id: "work.context".into(),
        source_handle: "work:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        coverage_complete: true,
        scope_handle: "workspace:selected-project".into(),
        items: vec![WorkContextItem {
            evidence_handle: "project:issue-42".into(),
            kind: WorkItemKind::Project,
            title: "Release readiness".into(),
            excerpt: Some("Launch review is waiting on API evidence.".into()),
            status: Some("blocked".into()),
            blocker: Some("Missing API evidence".into()),
            next_action: Some("Attach the validation result".into()),
            observed_at_unix_ms: NOW,
        }],
    }
}

fn logistics() -> LogisticsView {
    LogisticsView {
        schema_version: AGENT_VERSION,
        view_id: "life.logistics".into(),
        source_handle: "logistics:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        coverage_complete: true,
        items: vec![LogisticsItem {
            evidence_handle: "delivery:parcel".into(),
            kind: LogisticsItemKind::Delivery,
            summary: "Parcel delivery window".into(),
            status: "arriving_today".into(),
            occurs_at_unix_ms: Some(NOW + 3_600_000),
            needs_attention: true,
        }],
    }
}

#[test]
fn selected_work_and_logistics_views_are_bounded_and_source_linked() {
    let work = work();
    let logistics = logistics();
    validate_work_context_view(&work, NOW).unwrap();
    validate_logistics_view(&logistics, NOW).unwrap();
    assert!(
        work_context_evidence(&work)
            .unwrap()
            .untrusted_text
            .contains("blocked")
    );
    assert!(
        logistics_context_evidence(&logistics)
            .unwrap()
            .untrusted_text
            .contains("arriving_today")
    );
}

#[test]
fn unrestricted_or_high_authority_payload_fields_fail_closed() {
    for field in ["absolute_path", "organization_search", "full_document"] {
        let mut value = serde_json::to_value(work()).unwrap();
        value[field] = serde_json::json!("private");
        assert!(serde_json::from_value::<WorkContextView>(value).is_err());
    }
    for field in ["payment", "door_code", "unlock", "raw_webhook"] {
        let mut value = serde_json::to_value(logistics()).unwrap();
        value[field] = serde_json::json!("private");
        assert!(serde_json::from_value::<LogisticsView>(value).is_err());
    }
    let mut duplicate = logistics();
    duplicate.items.push(duplicate.items[0].clone());
    assert_eq!(
        validate_logistics_view(&duplicate, NOW),
        Err(AgentFailure::InvalidInput)
    );
}

#[test]
fn github_work_view_crosses_the_go_rust_contract() {
    let view: WorkContextView = serde_json::from_str(include_str!(
        "../../../server/internal/connectors/github/testdata/work_context.json"
    ))
    .unwrap();
    validate_work_context_view(&view, 1_789_012_800_000).unwrap();
    assert_eq!(view.items[0].kind, WorkItemKind::Project);
    assert_eq!(view.items[0].title, "Release readiness");
}

#[test]
fn home_assistant_logistics_view_crosses_the_go_rust_contract() {
    let view: LogisticsView = serde_json::from_str(include_str!(
        "../../../server/internal/connectors/homeassistant/testdata/logistics_view.json"
    ))
    .unwrap();
    validate_logistics_view(&view, 1_789_012_800_000).unwrap();
    assert_eq!(view.items[0].kind, LogisticsItemKind::HomeState);
    assert_eq!(view.items[0].summary, "Front door temperature");
}

#[test]
fn slack_work_view_crosses_the_go_rust_contract() {
    let view: WorkContextView = serde_json::from_str(include_str!(
        "../../../server/internal/connectors/slack/testdata/work_context.json"
    ))
    .unwrap();
    validate_work_context_view(&view, 1_789_128_000_000).unwrap();
    assert_eq!(view.items[0].kind, WorkItemKind::Communication);
    assert_eq!(view.items[0].title, "Release review");
}
