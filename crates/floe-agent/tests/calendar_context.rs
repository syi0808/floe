use floe_agent::{
    AgentFailure, CalendarContextView, DataClass, calendar_context_evidence,
    validate_calendar_context_view,
};

const NOW: i64 = 1_789_128_000_000;

fn fixture() -> CalendarContextView {
    serde_json::from_str(include_str!(
        "../../../server/internal/connectors/googlecalendar/testdata/calendar_view.json"
    ))
    .unwrap()
}

fn microsoft_fixture() -> CalendarContextView {
    serde_json::from_str(include_str!(
        "../../../server/internal/connectors/microsoftcalendar/testdata/calendar_view.json"
    ))
    .unwrap()
}

#[test]
fn google_calendar_view_crosses_the_strict_context_boundary() {
    let view = fixture();
    validate_calendar_context_view(&view, NOW).unwrap();
    let evidence = calendar_context_evidence(&view).unwrap();
    assert_eq!(evidence.data_class, DataClass::Personal);
    assert!(evidence.untrusted_text.contains("Planning review"));
    assert!(!evidence.untrusted_text.contains("provider-event-id"));
}

#[test]
fn microsoft_calendar_view_crosses_the_same_strict_context_boundary() {
    let view = microsoft_fixture();
    validate_calendar_context_view(&view, NOW).unwrap();
    let evidence = calendar_context_evidence(&view).unwrap();
    assert_eq!(evidence.data_class, DataClass::Personal);
    assert!(
        evidence
            .untrusted_text
            .contains("Microsoft planning review")
    );
    assert!(!evidence.untrusted_text.contains("provider-event-id"));
}

#[test]
fn calendar_context_rejects_stale_duplicate_and_escalated_shapes() {
    let mut stale = fixture();
    stale.expires_at_unix_ms = NOW;
    assert_eq!(
        validate_calendar_context_view(&stale, NOW),
        Err(AgentFailure::InvalidInput)
    );

    let mut duplicate = fixture();
    duplicate.items.push(duplicate.items[0].clone());
    assert_eq!(
        validate_calendar_context_view(&duplicate, NOW),
        Err(AgentFailure::InvalidInput)
    );

    let mut value = serde_json::to_value(fixture()).unwrap();
    value["authority"] = serde_json::json!("create");
    assert!(serde_json::from_value::<CalendarContextView>(value).is_err());
}
