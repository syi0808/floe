//! Synthetic package-finalization tests; no provider, grant or model dispatch.
use floe_agent_contract::{
    AgentContext, ArtifactPart, DataClass, DelegationExecutionContext, DelegationRequest,
    DependencyCoverage, InvocationKey, OwnerActor, TaskId, ToolCall,
};
use floe_context_contract::{CommunicationItem, CommunicationView};
use floe_experts::{
    ExpertAdmissionIdentity, ExpertExecutionSelection, ExpertPrivateState, ExpertProgram,
    ExpertProgramRequest, ExpertSourceObservation, ExpertToolObservation,
};
use serde_json::{Value, json};
use uuid::Uuid;

const NOW: i64 = 1_000_000;
fn request(kind: crate::BuiltinExpertKind) -> ExpertProgramRequest {
    let manifest = crate::manifests()
        .into_iter()
        .find(|m| m.package.id == kind.package_id())
        .unwrap();
    let person_id = Uuid::new_v4();
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        memories: vec![],
        optional_context_issues: vec![],
        evidence: vec![],
    };
    let actor = OwnerActor {
        person_id: floe_agent_contract::PersonId(person_id),
        device_id: "fixture-device".into(),
        runtime_epoch: 1,
    };
    let admission = ExpertAdmissionIdentity {
        registry_instance_id: Uuid::new_v4(),
        assignment_id: Uuid::new_v4(),
        installation_id: Uuid::new_v4(),
        package: manifest.package.clone(),
        definition_revision: manifest.definition.definition_revision,
    };
    let delegation = DelegationRequest {
        task_id: TaskId::new(),
        parent_run_id: Some(Uuid::new_v4()),
        principal: person_id.to_string(),
        invocation_key: InvocationKey::new(),
        selected_agent_id: manifest.package.id,
        selected_definition_revision: admission.definition_revision,
        message: "Assess the observed messages.".into(),
        context_refs: vec![],
        execution_context: DelegationExecutionContext {
            session_id: Uuid::new_v4(),
            device_id: actor.device_id.clone(),
            agent_context: context.clone(),
            max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
            projection_coverage: DependencyCoverage::Independent,
        },
    };
    ExpertProgramRequest {
        actor,
        request: delegation,
        admission,
        selection: ExpertExecutionSelection::without_requirements(1).unwrap(),
        private_state: ExpertPrivateState::default(),
        now_unix_ms: NOW,
        context,
        coverage: DependencyCoverage::Independent,
        started_at_unix_ms: NOW - 100,
        state_schema_version: 1,
        data_class: DataClass::Personal,
    }
}
fn mail(handle: &str, source: &str) -> CommunicationView {
    CommunicationView {
        schema_version: 1,
        view_id: "mail.communication".into(),
        source_handle: source.into(),
        observed_at_unix_ms: NOW - 100,
        expires_at_unix_ms: NOW + 10_000,
        coverage_complete: false,
        next_cursor: Some(1),
        items: vec![CommunicationItem {
            evidence_handle: handle.into(),
            thread_handle: format!("thread:{handle}"),
            received_unix_ms: NOW - 200,
            from: "sender@example.test".into(),
            to: "owner@example.test".into(),
            subject: "Review requested".into(),
            snippet: "Please review.".into(),
            labels: vec!["INBOX".into()],
        }],
    }
}
fn observation(view: CommunicationView, query: &str, cursor: usize) -> ExpertToolObservation {
    ExpertToolObservation {
        call: ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: InvocationKey::new(),
            tool_id: "floe.source.mail".into(),
            definition_revision: 1,
            input: json!({"schema_version":1,"query":query,"cursor":cursor,"limit":25}).to_string(),
        },
        requirement_key: "floe.source.mail".into(),
        outcome: ExpertSourceObservation::Ready {
            payload: serde_json::to_value(view).unwrap(),
            coverage: DependencyCoverage::Independent,
        },
    }
}
fn communication_output(handles: &[&str]) -> String {
    json!({"summary":"Observed review requests.","assessments":handles.iter().map(|handle| json!({
        "evidence_handle":handle,"needs_reply":true,"rationale":"A reply is requested.",
        "channel":"email","tone":"concise","draft":null})).collect::<Vec<_>>()})
    .to_string()
}
fn commitment_output(handles: &[&str]) -> String {
    json!({"summary":"Observed requests.","findings":handles.iter().map(|handle| json!({
        "evidence_handle":handle,"kind":"request_to_user","statement":"Review the message.",
        "epistemic_status":"observed","confidence_millis":1000,"deadline_unix_ms":null})).collect::<Vec<_>>()}).to_string()
}
fn artifact(output: floe_experts::ExpertFinalOutput) -> Value {
    let ArtifactPart::Data { data, .. } = &output.payload.artifacts[0].parts[0] else {
        panic!("typed result expected")
    };
    serde_json::from_str(data).unwrap()
}

#[test]
fn mail_two_pages_finalize_with_both_exact_observed_handles() {
    let observations = vec![
        observation(mail("mail:a", "snapshot:a"), "in:inbox", 0),
        observation(mail("mail:b", "snapshot:b"), "in:inbox", 1),
    ];
    let communication = crate::communication::CommunicationProgram
        .finalize(
            &request(crate::BuiltinExpertKind::Communication),
            &observations,
            &communication_output(&["mail:a", "mail:b"]),
            &[],
        )
        .expect("two actual pages are valid evidence");
    assert_eq!(
        artifact(communication)["assessments"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let commitments = crate::commitments::CommitmentsProgram
        .finalize(
            &request(crate::BuiltinExpertKind::Commitments),
            &observations,
            &commitment_output(&["mail:a", "mail:b"]),
            &[],
        )
        .expect("both mail pages can support findings");
    assert_eq!(
        artifact(commitments)["findings"].as_array().unwrap().len(),
        2
    );
}

fn mail_result(
    observations: &[ExpertToolObservation],
    handles: &[&str],
) -> Result<Value, floe_agent_contract::AgentFailure> {
    crate::communication::CommunicationProgram
        .finalize(
            &request(crate::BuiltinExpertKind::Communication),
            observations,
            &communication_output(handles),
            &[],
        )
        .map(artifact)
}
fn failed(mut read: ExpertToolObservation) -> ExpertToolObservation {
    read.call.call_id = Uuid::new_v4();
    read.call.invocation_key = InvocationKey::new();
    read.outcome = ExpertSourceObservation::Unavailable {
        reason: floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
    };
    read
}
#[test]
fn mail_refined_overlap_preserves_sources_and_conservative_expiry() {
    let a = mail("mail:a", "snapshot:a");
    let mut b = a.clone();
    b.source_handle = "snapshot:b".into();
    b.expires_at_unix_ms += 500;
    let result = mail_result(
        &[
            observation(a, "in:inbox", 0),
            observation(b, "is:important", 0),
        ],
        &["mail:a"],
    )
    .unwrap();
    assert_eq!(
        result["source_handles"],
        json!(["snapshot:a", "snapshot:b"])
    );
    assert_eq!(result["expires_at_unix_ms"], NOW + 10_000);
    assert_eq!(result["assessments"].as_array().unwrap().len(), 1);
    assert!(
        result.get("source_handle").is_none(),
        "no fabricated singleton source"
    );
    assert!(
        result.get("coverage_complete").is_none(),
        "union does not claim provider completeness"
    );
}
#[test]
fn exact_normalized_query_refresh_supersedes_only_that_page() {
    let mut expired = mail("mail:old", "expired");
    expired.expires_at_unix_ms = NOW - 1;
    let old = observation(expired, "in:inbox", 0);
    let other = observation(mail("mail:other", "other-page"), "in:inbox", 1);
    let mut refreshed = observation(mail("mail:new", "refreshed"), "in:inbox", 0);
    refreshed.call.input =
        "{ \"limit\":25, \"cursor\":0, \"query\":\"in:inbox\", \"schema_version\":1 }".into();
    let observations = vec![old, other, refreshed];
    let result = mail_result(&observations, &["mail:new", "mail:other"]).unwrap();
    assert_eq!(result["source_handles"], json!(["other-page", "refreshed"]));
    assert!(
        mail_result(&observations, &["mail:old"]).is_err(),
        "refreshed query cannot resurrect removed evidence"
    );
}
#[test]
fn failed_query_refresh_drops_only_its_previous_success() {
    let first = observation(mail("mail:a", "first"), "in:inbox", 0);
    let other = observation(mail("mail:b", "second"), "in:inbox", 1);
    let observations = vec![first.clone(), other, failed(first.clone())];
    assert!(mail_result(&observations, &["mail:b"]).is_ok());
    assert!(mail_result(&observations, &["mail:a"]).is_err());
    let unavailable = mail_result(&[first.clone(), failed(first.clone())], &["mail:a"]).unwrap();
    assert!(
        unavailable.get("assessments").is_none(),
        "unavailable result must not contain the old assessment"
    );
    assert!(mail_result(&[failed(first.clone()), first], &["mail:a"]).is_ok());
}
#[test]
fn changed_overlap_uses_newer_observation_but_equal_timestamp_conflict_fails() {
    let first = mail("mail:a", "first");
    let mut revised = first.clone();
    revised.source_handle = "revised".into();
    revised.items[0].snippet = "Updated review request".into();
    let initial = observation(first, "in:inbox", 0);
    assert!(matches!(
        mail_result(
            &[
                initial.clone(),
                observation(revised.clone(), "is:important", 0)
            ],
            &["mail:a"]
        ),
        Err(floe_agent_contract::AgentFailure::Conflict)
    ));
    revised.observed_at_unix_ms += 1;
    let observations = [initial, observation(revised, "is:important", 0)];
    let result = crate::commitments::CommitmentsProgram
        .finalize(
            &request(crate::BuiltinExpertKind::Commitments),
            &observations,
            &commitment_output(&["mail:a"]),
            &[],
        )
        .map(artifact)
        .unwrap();
    assert_eq!(result["findings"][0]["source_handle"], "revised");
    assert_eq!(result["source_handles"], json!(["first", "revised"]));
}
#[test]
fn mail_rejects_unknown_citations_expired_pages_and_unknown_coverage() {
    let observation = observation(mail("mail:a", "first"), "in:inbox", 0);
    assert!(mail_result(&[observation.clone()], &["invented"]).is_err());
    assert!(
        crate::commitments::CommitmentsProgram
            .finalize(
                &request(crate::BuiltinExpertKind::Commitments),
                &[observation.clone()],
                &commitment_output(&["invented"]),
                &[]
            )
            .is_err()
    );
    let mut expired = mail("mail:b", "expired");
    expired.expires_at_unix_ms = NOW;
    assert!(
        mail_result(
            &[
                observation.clone(),
                self::observation(expired, "in:inbox", 1)
            ],
            &["mail:a"]
        )
        .is_err()
    );
    let mut request = request(crate::BuiltinExpertKind::Communication);
    request.coverage = DependencyCoverage::Unknown;
    assert!(matches!(
        crate::communication::CommunicationProgram.finalize(
            &request,
            &[observation],
            &communication_output(&["mail:a"]),
            &[]
        ),
        Err(floe_agent_contract::AgentFailure::PolicyDenied)
    ));
}
#[test]
fn observations_and_each_page_keep_the_existing_capacity_bounds() {
    let observations = (0..=floe_experts::MAX_EXPERT_SOURCE_OBSERVATIONS)
        .map(|cursor| observation(mail("mail:a", "snapshot"), "in:inbox", cursor))
        .collect::<Vec<_>>();
    assert!(matches!(
        mail_result(&observations, &["mail:a"]),
        Err(floe_agent_contract::AgentFailure::BudgetExceeded)
    ));
    let mut excessive = mail("mail:a", "large");
    excessive.items = (0..101)
        .map(|n| {
            let mut item = excessive.items[0].clone();
            item.evidence_handle = format!("mail:{n}");
            item
        })
        .collect();
    assert!(matches!(
        mail_result(&[observation(excessive, "in:inbox", 0)], &[]),
        Err(floe_agent_contract::AgentFailure::BudgetExceeded)
    ));
    let mut req = request(crate::BuiltinExpertKind::Communication);
    req.request.execution_context.max_output_bytes = 8;
    assert!(matches!(
        crate::communication::CommunicationProgram.finalize(
            &req,
            &[],
            &communication_output(&[]),
            &[]
        ),
        Err(floe_agent_contract::AgentFailure::BudgetExceeded)
    ));
}
#[test]
fn two_individually_bounded_mail_pages_do_not_gain_a_single_page_lifetime_cap() {
    let observations = (0..2)
        .map(|page| {
            let mut view = mail("seed", &format!("page:{page}"));
            view.items = (0..100)
                .map(|n| {
                    let mut item = view.items[0].clone();
                    item.evidence_handle = format!("mail:{}", page * 100 + n);
                    item
                })
                .collect();
            observation(view, "in:inbox", page)
        })
        .collect::<Vec<_>>();
    let result = mail_result(&observations, &["mail:0", "mail:199"]).unwrap();
    assert_eq!(result["assessments"].as_array().unwrap().len(), 2);
}
#[test]
fn full_snapshot_reread_observes_the_last_actual_outcome() {
    let first = observation(mail("mail:a", "first"), "", 0);
    let second = observation(mail("mail:b", "second"), "", 0);
    let read = |observations: &[ExpertToolObservation]| {
        crate::program_support::read_snapshot::<CommunicationView>(observations, "floe.source.mail")
    };
    assert_eq!(
        read(&[first.clone(), second.clone()])
            .unwrap()
            .unwrap()
            .source_handle,
        "second"
    );
    assert!(
        read(&[first.clone(), failed(second.clone())])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        read(&[failed(first), second])
            .unwrap()
            .unwrap()
            .source_handle,
        "second"
    );
}
fn calendar(handle: &str, source: &str, cursor: usize) -> ExpertToolObservation {
    let view = floe_context_contract::CalendarContextView {
        schema_version: 1,
        view_id: "calendar.timeline".into(),
        source_handle: source.into(),
        observed_at_unix_ms: NOW - 100,
        expires_at_unix_ms: NOW + 5000,
        range_start_unix_ms: NOW - 1000,
        range_end_unix_ms: NOW + 100_000,
        coverage_complete: false,
        next_cursor: Some("next".into()),
        items: vec![floe_context_contract::CalendarContextItem {
            evidence_handle: handle.into(),
            untrusted_title: "Review".into(),
            starts_at_unix_ms: NOW,
            ends_at_unix_ms: NOW + 1000,
            all_day: false,
        }],
    };
    ExpertToolObservation { call: ToolCall { call_id: Uuid::new_v4(), invocation_key: InvocationKey::new(),
        tool_id: "floe.source.calendar".into(), definition_revision: 1,
        input: json!({"range_start_unix_ms": NOW-1000,"range_end_unix_ms":NOW+100_000,"cursor":format!("page:{cursor}"),"limit":25}).to_string() },
        requirement_key:"floe.source.calendar".into(), outcome: ExpertSourceObservation::Ready {
            payload: json!([view]), coverage:DependencyCoverage::Independent } }
}
#[test]
fn commitment_calendar_pages_deduplicate_overlap_and_keep_exact_origins() {
    let observations = vec![
        observation(mail("mail:a", "mail"), "in:inbox", 0),
        calendar("calendar:a", "calendar-page:a", 0),
        calendar("calendar:a", "calendar-page:b", 1),
        calendar("calendar:b", "calendar-page:c", 2),
    ];
    let result = crate::commitments::CommitmentsProgram
        .finalize(
            &request(crate::BuiltinExpertKind::Commitments),
            &observations,
            &commitment_output(&["mail:a", "calendar:a", "calendar:b"]),
            &[],
        )
        .map(artifact)
        .unwrap();
    assert_eq!(result["findings"].as_array().unwrap().len(), 3);
    assert_eq!(result["expires_at_unix_ms"], NOW + 5000);
    assert_eq!(result["source_handles"].as_array().unwrap().len(), 4);
    assert_eq!(result["findings"][2]["source_handle"], "calendar-page:c");
    let collision = vec![
        observation(mail("same", "mail"), "in:inbox", 0),
        calendar("same", "calendar", 0),
    ];
    assert!(
        crate::commitments::CommitmentsProgram
            .finalize(
                &request(crate::BuiltinExpertKind::Commitments),
                &collision,
                &commitment_output(&["same"]),
                &[]
            )
            .is_err(),
        "cross-domain handle collisions are not deduplication"
    );
}

fn snapshot(key: &str, payload: Value) -> ExpertToolObservation {
    ExpertToolObservation {
        call: ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: InvocationKey::new(),
            tool_id: key.into(),
            definition_revision: 1,
            input: "{\"schema_version\":1}".into(),
        },
        requirement_key: key.into(),
        outcome: ExpertSourceObservation::Ready {
            payload,
            coverage: DependencyCoverage::Independent,
        },
    }
}
#[test]
fn focus_and_wellbeing_accept_refreshed_snapshots_and_overlapping_calendar_pages() {
    let calendar_reads = vec![
        calendar("calendar:a", "calendar-page:a", 0),
        calendar("calendar:a", "calendar-page:b", 1),
        calendar("calendar:b", "calendar-page:c", 2),
    ];
    let attention = |handle: &str| {
        snapshot(
            "floe.source.attention",
            json!({"schema_version":1,"view_id":"attention.coarse",
        "source_handle":handle,"observed_at_unix_ms":NOW-100,"expires_at_unix_ms":NOW+10_000,
        "state":"focused","confidence_millis":1000,"evidence_handles":[handle]}),
        )
    };
    let mut observations = vec![attention("attention:old"), attention("attention:new")];
    observations.extend(calendar_reads.clone());
    let judgment = |handles: Vec<&str>| {
        json!({"summary":"Protect the current focus.","recommendation":"protect_focus",
        "rationale":"Observed focus and calendar evidence.","evidence_handles":handles})
        .to_string()
    };
    let result = crate::focus_attention::FocusAttentionProgram
        .finalize(
            &request(crate::BuiltinExpertKind::FocusAttention),
            &observations,
            &judgment(vec!["attention:new", "calendar:a", "calendar:b"]),
            &[],
        )
        .map(artifact)
        .unwrap();
    assert_eq!(result["source_handle"], "attention:new");
    assert_eq!(result["source_handles"].as_array().unwrap().len(), 4);
    assert!(
        crate::focus_attention::FocusAttentionProgram
            .finalize(
                &request(crate::BuiltinExpertKind::FocusAttention),
                &observations,
                &judgment(vec!["attention:old"]),
                &[]
            )
            .is_err()
    );
    let wellbeing = |handle: &str| {
        snapshot(
            "floe.source.wellbeing",
            json!({"schema_version":1,"view_id":"wellbeing.derived",
        "source_handle":handle,"observed_at_unix_ms":NOW-100,"expires_at_unix_ms":NOW+10_000,
        "capacity":"typical","recovery":"typical","confidence_millis":1000,"evidence_handles":[handle]}),
        )
    };
    let mut observations = vec![wellbeing("wellbeing:old"), wellbeing("wellbeing:new")];
    observations.extend(calendar_reads);
    let text = json!({"summary":"Keep the plan.","schedule_impact":"keep_plan","rationale":"Derived capacity and calendar evidence.",
        "evidence_handles":["wellbeing:new","calendar:a","calendar:b"]}).to_string();
    let result = crate::wellbeing::WellbeingProgram
        .finalize(
            &request(crate::BuiltinExpertKind::Wellbeing),
            &observations,
            &text,
            &[],
        )
        .map(artifact)
        .unwrap();
    assert_eq!(result["source_handle"], "wellbeing:new");
    assert_eq!(result["source_handles"].as_array().unwrap().len(), 4);
}
#[test]
fn changed_packages_advance_definition_and_communication_result_contract() {
    for manifest in crate::manifests() {
        manifest.validate().unwrap();
        assert_eq!(
            manifest.definition.definition_revision,
            if manifest.package.id == "floe.builtin.schedule" {
                2
            } else {
                3
            }
        );
        assert_eq!(
            manifest.result_contracts[0].revision,
            if manifest.package.id == "floe.builtin.communication" {
                2
            } else {
                1
            }
        );
    }
}
#[test]
fn selected_payload_budget_applies_to_pages_and_snapshots() {
    let read = snapshot(
        "large",
        json!({"text":"x".repeat(floe_agent_contract::MAX_OUTPUT_BYTES)}),
    );
    assert!(matches!(
        crate::program_support::read_snapshot::<Value>(&[read.clone()], "large"),
        Err(floe_agent_contract::AgentFailure::BudgetExceeded)
    ));
    assert!(matches!(
        crate::program_support::read_pages::<Value>(&[read], "large"),
        Err(floe_agent_contract::AgentFailure::BudgetExceeded)
    ));
}
