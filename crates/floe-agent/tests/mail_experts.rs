use std::{collections::VecDeque, sync::Mutex};

use floe_agent::{
    AGENT_VERSION, AgentContext, AgentFailure, COMMITMENTS_AGGREGATE_SOURCE_HANDLE,
    CalendarContextItem, CalendarContextView, Cancellation, CommitmentEvidenceSource,
    CommitmentsContextViews, CommunicationItem, CommunicationResultKind, CommunicationView,
    ContextMemory, DataClass, EpistemicStatus, FindingEpistemicStatus, InferencePolicyDecision,
    LearningEvidenceRef, MailExpertInvocation, ModelPlacement, ModelRequest, ModelResponse,
    ModelRunner, ModelStep, NativeContextItem, NativeContextView, PersonalMemoryKind, PromptRole,
    TaskContextPriority, TransferConsent, UsageLedger, run_commitments_expert,
    run_commitments_expert_with_views, run_communication_expert,
};
use floe_domain::PersonId;
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

const NOW: i64 = 1_789_000_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    id: String,
    assignment: String,
    item: CommunicationItem,
    commitments_output: Value,
    communication_output: Value,
    expected_commitments: usize,
    expected_reply: bool,
}

struct Model {
    placement: ModelPlacement,
    outputs: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl Model {
    fn new(outputs: impl IntoIterator<Item = Value>) -> Self {
        Self {
            placement: ModelPlacement::DeviceLocal,
            outputs: Mutex::new(outputs.into_iter().map(|value| value.to_string()).collect()),
            requests: Mutex::new(vec![]),
        }
    }
}

impl ModelRunner for Model {
    fn placement(&self) -> ModelPlacement {
        self.placement
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        self.requests.lock().unwrap().push(request);
        Ok(ModelResponse {
            replay: None,
            schema_version: AGENT_VERSION,
            output: vec![ModelStep::Answer {
                text: self.outputs.lock().unwrap().pop_front().unwrap(),
            }],
            used_tokens: 64,
            cost_micros: 0,
        })
    }
}

fn invocation(assignment: &str, item: CommunicationItem) -> MailExpertInvocation {
    MailExpertInvocation {
        usage: UsageLedger::default(),
        person_id: PersonId::new(),
        invocation_id: Uuid::new_v4(),
        assignment: assignment.into(),
        current_time_unix_ms: NOW,
        context: AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        },
        view: CommunicationView {
            schema_version: AGENT_VERSION,
            view_id: "mail.communication".into(),
            source_handle: "mail:corpus".into(),
            observed_at_unix_ms: NOW,
            expires_at_unix_ms: NOW + 300_000,
            coverage_complete: true,
            next_cursor: None,
            items: vec![item],
        },
        max_output_bytes: 8192,
        max_model_tokens: 4096,
        max_model_cost_micros: 10_000,
        deadline: Instant::now() + Duration::from_secs(5),
        cancellation: Cancellation::default(),
    }
}

fn policy() -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "everyday-assistance".into(),
        data_classes: vec![floe_agent::DataClass::Personal],
        allowed_placements: vec![ModelPlacement::DeviceLocal],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

#[tokio::test]
async fn commitments_and_communication_corpus_preserve_evidence_and_authority() {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("fixtures/mail_expert_corpus.json")).unwrap();
    for scenario in scenarios {
        let model = Model::new([scenario.commitments_output, scenario.communication_output]);
        let commitments = run_commitments_expert(
            &model,
            &policy(),
            invocation(&scenario.assignment, scenario.item.clone()),
        )
        .await
        .unwrap_or_else(|error| panic!("{} commitments: {error:?}", scenario.id));
        let communication = run_communication_expert(
            &model,
            &policy(),
            invocation(&scenario.assignment, scenario.item),
        )
        .await
        .unwrap_or_else(|error| panic!("{} communication: {error:?}", scenario.id));
        assert_eq!(commitments.findings.len(), scenario.expected_commitments);
        assert_eq!(commitments.source_handle, "mail:corpus");
        assert_eq!(commitments.source_handles, ["mail:corpus"]);
        assert_eq!(
            communication.assessments[0].needs_reply,
            scenario.expected_reply
        );
        assert!(communication.assessments[0].draft.is_none() || scenario.expected_reply);
        assert_eq!(
            communication.assessments[0].result,
            if communication.assessments[0].draft.is_some() {
                CommunicationResultKind::DraftForReview
            } else if scenario.expected_reply {
                CommunicationResultKind::ReplyRecommended
            } else {
                CommunicationResultKind::NoReply
            }
        );
        assert_eq!(
            communication.assessments[0].requires_review,
            communication.assessments[0].draft.is_some()
        );
        let requests = model.requests.lock().unwrap();
        assert_eq!(requests[0].prompt.role, PromptRole::CommitmentsExpert);
        assert_eq!(requests[1].prompt.role, PromptRole::CommunicationExpert);
        for request in requests.iter() {
            assert_eq!(request.context.evidence.len(), 1);
            assert!(request.capabilities.is_empty());
            assert!(
                request.context.evidence[0]
                    .untrusted_text
                    .contains("subject")
            );
            assert!(!request.prompt.render().contains("mail.send"));
        }
    }
}

#[tokio::test]
async fn commitments_accept_bounded_multi_source_evidence_without_blurring_sources() {
    let mail_item = CommunicationItem {
        evidence_handle: "mail:evidence".into(),
        thread_handle: "mail:thread".into(),
        received_unix_ms: NOW - 1,
        from: "alex@example.com".into(),
        to: "person@example.com".into(),
        subject: "Project follow-up".into(),
        snippet: "Please reply this week.".into(),
        labels: vec!["INBOX".into()],
    };
    let mut invocation = invocation("Assess all bounded commitment evidence", mail_item);
    let person_id = invocation.person_id;
    let memory_id = Uuid::new_v4();
    invocation.context.memories.push(ContextMemory {
        target_id: memory_id,
        revision: 1,
        kind: PersonalMemoryKind::Commitment,
        statement: "The user confirmed a Friday delivery.".into(),
        epistemic_status: EpistemicStatus::Fact,
        confidence_millis: 1000,
        observed_at_unix_ms: NOW - 1_000,
        valid_from_unix_ms: None,
        valid_until_unix_ms: Some(NOW + 200_000),
        source_refs: vec![LearningEvidenceRef {
            session_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
        }],
    });
    let task_id = Uuid::new_v4();
    let views = CommitmentsContextViews {
        calendars: vec![CalendarContextView {
            schema_version: AGENT_VERSION,
            view_id: "calendar.timeline".into(),
            source_handle: "calendar:primary".into(),
            observed_at_unix_ms: NOW,
            expires_at_unix_ms: NOW + 250_000,
            range_start_unix_ms: NOW - 86_400_000,
            range_end_unix_ms: NOW + 86_400_000,
            coverage_complete: true,
            next_cursor: None,
            items: vec![CalendarContextItem {
                evidence_handle: "calendar:event".into(),
                untrusted_title: "Delivery review".into(),
                starts_at_unix_ms: NOW + 10_000,
                ends_at_unix_ms: NOW + 20_000,
                all_day: false,
            }],
        }],
        tasks: vec![NativeContextView {
            schema_version: AGENT_VERSION,
            handle: Uuid::new_v4(),
            person_id,
            view_id: "floe.tasks".into(),
            data_class: DataClass::Personal,
            source_handle: "floe:tasks".into(),
            observed_at_unix_ms: NOW as u64,
            expires_at_unix_ms: (NOW + 240_000) as u64,
            coverage_complete: true,
            next_cursor: None,
            items: vec![NativeContextItem::Task {
                evidence_handle: task_id,
                untrusted_title: "Prepare delivery".into(),
                deadline_unix_ms: Some((NOW + 30_000) as u64),
                priority: TaskContextPriority::High,
            }],
        }],
    };
    let model = Model::new([serde_json::json!({
        "summary": "Four bounded sources contain commitment evidence.",
        "findings": [
            {"evidence_handle":"mail:evidence","kind":"request_to_user","statement":"A reply was requested.","epistemic_status":"observed","confidence_millis":1000},
            {"evidence_handle":"calendar:event","kind":"user_commitment","statement":"A review is scheduled.","epistemic_status":"observed","confidence_millis":1000},
            {"evidence_handle":task_id.to_string(),"kind":"user_commitment","statement":"A delivery task exists.","epistemic_status":"observed","confidence_millis":1000},
            {"evidence_handle":memory_id.to_string(),"kind":"user_commitment","statement":"Friday was previously confirmed.","epistemic_status":"observed","confidence_millis":1000}
        ]
    })]);

    let result = run_commitments_expert_with_views(&model, &policy(), invocation, views)
        .await
        .unwrap();

    assert_eq!(result.expires_at_unix_ms, NOW + 200_000);
    assert_eq!(result.source_handle, COMMITMENTS_AGGREGATE_SOURCE_HANDLE);
    assert_eq!(
        result.findings[0].evidence_source,
        CommitmentEvidenceSource::Mail
    );
    assert_eq!(
        result.findings[1].evidence_source,
        CommitmentEvidenceSource::Calendar
    );
    assert_eq!(
        result.findings[2].evidence_source,
        CommitmentEvidenceSource::FloeTask
    );
    assert_eq!(
        result.findings[3].evidence_source,
        CommitmentEvidenceSource::ConfirmedMemory
    );
    assert_eq!(result.findings[1].source_handle, "calendar:primary");
    assert!(result.source_handles.contains(&"mail:corpus".into()));
    let round_trip: floe_agent::CommitmentsExpertResult =
        serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(round_trip, result);
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests[0].context.evidence.len(), 3);
    assert_eq!(requests[0].context.memories.len(), 1);
    assert!(requests[0].capabilities.is_empty());
}

#[tokio::test]
async fn calendar_only_evidence_uses_calendar_as_the_compatibility_source() {
    let mut invocation = invocation(
        "Assess the calendar commitment",
        CommunicationItem {
            evidence_handle: "unused:mail".into(),
            thread_handle: "unused:thread".into(),
            received_unix_ms: NOW - 1,
            from: String::new(),
            to: String::new(),
            subject: String::new(),
            snippet: String::new(),
            labels: vec![],
        },
    );
    invocation.view.items.clear();
    let views = CommitmentsContextViews {
        calendars: vec![CalendarContextView {
            schema_version: AGENT_VERSION,
            view_id: "calendar.timeline".into(),
            source_handle: "calendar:only".into(),
            observed_at_unix_ms: NOW,
            expires_at_unix_ms: NOW + 250_000,
            range_start_unix_ms: NOW - 86_400_000,
            range_end_unix_ms: NOW + 86_400_000,
            coverage_complete: true,
            next_cursor: None,
            items: vec![CalendarContextItem {
                evidence_handle: "calendar:commitment".into(),
                untrusted_title: "Submit report".into(),
                starts_at_unix_ms: NOW + 10_000,
                ends_at_unix_ms: NOW + 20_000,
                all_day: false,
            }],
        }],
        tasks: vec![],
    };
    let model = Model::new([serde_json::json!({
        "summary": "The calendar contains one commitment.",
        "findings": [{
            "evidence_handle":"calendar:commitment",
            "kind":"user_commitment",
            "statement":"A report submission is scheduled.",
            "epistemic_status":"observed",
            "confidence_millis":1000
        }]
    })]);

    let result = run_commitments_expert_with_views(&model, &policy(), invocation, views)
        .await
        .unwrap();

    assert_eq!(result.source_handle, "calendar:only");
    assert_eq!(result.source_handles, ["calendar:only"]);
    assert_eq!(result.expires_at_unix_ms, NOW + 250_000);
    assert_eq!(result.findings[0].source_handle, "calendar:only");
    assert_eq!(
        result.findings[0].evidence_source,
        CommitmentEvidenceSource::Calendar
    );
}

#[tokio::test]
async fn expert_outputs_cannot_invent_evidence_or_blur_inference() {
    let item = CommunicationItem {
        evidence_handle: "mail:evidence".into(),
        thread_handle: "mail:thread".into(),
        received_unix_ms: NOW - 1,
        from: "alex@example.com".into(),
        to: "person@example.com".into(),
        subject: "Possible follow-up".into(),
        snippet: "Maybe check in next week.".into(),
        labels: vec!["INBOX".into()],
    };
    for output in [
        serde_json::json!({
            "summary": "Invented",
            "findings": [{
                "evidence_handle": "mail:missing",
                "kind": "follow_up_gap",
                "statement": "Follow up.",
                "epistemic_status": "inferred",
                "confidence_millis": 700
            }]
        }),
        serde_json::json!({
            "summary": "Overstated",
            "findings": [{
                "evidence_handle": "mail:evidence",
                "kind": "follow_up_gap",
                "statement": "Follow up.",
                "epistemic_status": "observed",
                "confidence_millis": 700
            }]
        }),
    ] {
        let model = Model::new([output]);
        assert_eq!(
            run_commitments_expert(&model, &policy(), invocation("Assess", item.clone())).await,
            Err(AgentFailure::InvalidModelOutput)
        );
    }

    let model = Model::new([serde_json::json!({
        "summary": "No reply needed",
        "assessments": [{
            "evidence_handle": "mail:evidence",
            "needs_reply": false,
            "rationale": "No request was made.",
            "channel": "email",
            "tone": "neutral",
            "draft": "I will send this without approval."
        }]
    })]);
    assert_eq!(
        run_communication_expert(&model, &policy(), invocation("Assess", item)).await,
        Err(AgentFailure::InvalidModelOutput)
    );
}

#[test]
fn corpus_uses_observed_only_for_explicit_evidence() {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("fixtures/mail_expert_corpus.json")).unwrap();
    let output: Vec<floe_agent::CommitmentFinding> =
        serde_json::from_value(scenarios[0].commitments_output["findings"].clone()).unwrap();
    assert_eq!(output[0].epistemic_status, FindingEpistemicStatus::Observed);
    assert_eq!(output[0].confidence_millis, 1000);
}
