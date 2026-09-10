use floe_agent::*;

fn policy(class: DataClass) -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "bounded-test-projection".into(),
        data_classes: vec![class],
        allowed_placements: vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote],
        performance_class: "fast".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

fn context(class: DataClass) -> AgentContext {
    AgentContext {
        projection_version: 1,
        persona: None,
        memories: vec![],
        evidence: vec![ContextEvidence {
            source_handle: "synthetic:coarse-state".into(),
            data_class: class,
            untrusted_text: "Synthetic coarse state".into(),
            expires_at_unix_ms: 100,
        }],
    }
}

fn memory() -> ContextMemory {
    ContextMemory {
        target_id: uuid::Uuid::new_v4(),
        revision: 2,
        kind: PersonalMemoryKind::Preference,
        statement: "The user prefers afternoon meetings.".into(),
        epistemic_status: EpistemicStatus::Fact,
        confidence_millis: 1000,
        observed_at_unix_ms: 10,
        valid_from_unix_ms: None,
        valid_until_unix_ms: Some(200),
        source_refs: vec![LearningEvidenceRef {
            session_id: uuid::Uuid::new_v4(),
            turn_id: uuid::Uuid::new_v4(),
        }],
    }
}

#[test]
fn highly_sensitive_requires_both_bounded_projection_and_separate_remote_consent() {
    let mut policy = policy(DataClass::HighlySensitive);
    let context = context(DataClass::HighlySensitive);
    assert_eq!(
        policy.authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &context,
            1
        ),
        Ok(())
    );
    assert_eq!(
        policy.authorize(
            ModelPlacement::Remote,
            SessionProtection::Encrypted,
            &context,
            1
        ),
        Err(AgentFailure::ConsentRequired)
    );
    policy.bounded_sensitive_projection = true;
    assert_eq!(
        policy.authorize(
            ModelPlacement::Remote,
            SessionProtection::Encrypted,
            &context,
            1
        ),
        Err(AgentFailure::ConsentRequired)
    );
    policy.external_transfer_consent = TransferConsent::Granted;
    assert_eq!(
        policy.authorize(
            ModelPlacement::Remote,
            SessionProtection::Encrypted,
            &context,
            1
        ),
        Ok(())
    );
    policy.bounded_sensitive_projection = false;
    assert_eq!(
        policy.authorize(
            ModelPlacement::Remote,
            SessionProtection::Encrypted,
            &context,
            1
        ),
        Err(AgentFailure::PolicyDenied)
    );
}

#[test]
fn raw_sources_and_credentials_stay_outside_agent_context_even_with_consent() {
    for class in [DataClass::DeviceOnlyRaw, DataClass::Credential] {
        let mut policy = policy(class);
        policy.bounded_sensitive_projection = true;
        policy.external_transfer_consent = TransferConsent::Granted;
        for placement in [ModelPlacement::DeviceLocal, ModelPlacement::Remote] {
            assert_eq!(
                policy.authorize(placement, SessionProtection::Encrypted, &context(class), 1),
                Err(AgentFailure::PolicyDenied)
            );
        }
    }
}

#[test]
fn expired_and_empty_projection_metadata_fail_closed() {
    let mut policy = policy(DataClass::Personal);
    let context = context(DataClass::Personal);
    assert_eq!(
        policy.authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &context,
            100
        ),
        Err(AgentFailure::StaleContext)
    );
    policy.purpose.clear();
    assert_eq!(
        policy.authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &context,
            1
        ),
        Err(AgentFailure::PolicyDenied)
    );
    policy.purpose = "test".into();
    policy.projection_version = 0;
    assert_eq!(
        policy.authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &context,
            1
        ),
        Err(AgentFailure::PolicyDenied)
    );
}

#[test]
fn confirmed_memory_requires_personal_scope_and_current_bounded_metadata() {
    let mut personal = context(DataClass::Personal);
    personal.evidence.clear();
    personal.memories.push(memory());
    assert_eq!(
        policy(DataClass::Personal).authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &personal,
            100,
        ),
        Ok(())
    );
    assert_eq!(
        policy(DataClass::Synthetic).authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::SyntheticOnly,
            &personal,
            100,
        ),
        Err(AgentFailure::PolicyDenied)
    );
    personal.memories[0].valid_until_unix_ms = Some(100);
    assert_eq!(
        policy(DataClass::Personal).authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &personal,
            100,
        ),
        Err(AgentFailure::StaleContext)
    );
    personal.memories[0].valid_until_unix_ms = None;
    personal.memories[0].source_refs.clear();
    assert_eq!(
        policy(DataClass::Personal).authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &personal,
            100,
        ),
        Err(AgentFailure::PolicyDenied)
    );
}

#[test]
fn context_evidence_is_bounded_and_source_handles_are_unique() {
    let policy = policy(DataClass::Personal);
    let mut projected = context(DataClass::Personal);
    projected.evidence[0].untrusted_text = "x".repeat(MAX_CONTEXT_EVIDENCE_BYTES + 1);
    assert_eq!(
        policy.authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &projected,
            1,
        ),
        Err(AgentFailure::BudgetExceeded)
    );

    let duplicate = context(DataClass::Personal).evidence[0].clone();
    projected = context(DataClass::Personal);
    projected.evidence.push(duplicate);
    assert_eq!(
        policy.authorize(
            ModelPlacement::DeviceLocal,
            SessionProtection::Encrypted,
            &projected,
            1,
        ),
        Err(AgentFailure::PolicyDenied)
    );
}
