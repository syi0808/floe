//! Real App/Context/Inference/Vault capacity recovery with an in-process transport.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone)]
pub(crate) struct CapacityModel {
    limit: Arc<AtomicUsize>,
    calls: Arc<Mutex<Vec<(String, InferenceRequest)>>>,
    rejected: Arc<AtomicUsize>,
    failure_mode: Arc<AtomicUsize>,
    foreground_reply: String,
    compaction_barrier: Option<Arc<GenerateBarrier>>,
    plans: Arc<AtomicUsize>,
}
impl ModelProviderFactory for CapacityModel {
    fn create(&self, _: GatewayCredentialStore) -> Arc<dyn ModelProvider> {
        Arc::new(self.clone())
    }
}
impl ModelProvider for CapacityModel {
    fn observe_primary<'a>(
        &'a self,
        _: &'a ModelPlanRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<PrimaryObservation<Box<dyn PreparedModelTransport>>, ModelObservationError>,
    > {
        Box::pin(async move {
            self.plans.fetch_add(1, Ordering::SeqCst);
            Ok(PrimaryObservation::Absent(
                PrimaryAbsence::NoGatewayConfigured,
            ))
        })
    }
    fn observe_local_fallback<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<LocalObservation<Box<dyn PreparedModelTransport>>, ModelObservationError>,
    > {
        Box::pin(async move {
            self.plans.fetch_add(1, Ordering::SeqCst);
            if request.consumer == floe_conversation::COMPACTION_CONSUMER {
                assert!(scope.root_run_id().is_none());
                assert!(scope.task_id().is_none());
            } else {
                assert_eq!(request.consumer, CONVERSATION_CONSUMER);
                assert!(scope.root_run_id().is_some());
                assert!(scope.task_id().is_none());
            }
            Ok(LocalObservation::Available(PreparedModelProfile {
                capability: ObservedModelCapability {
                    purpose: ModelPurpose::new(request.purpose.clone()).unwrap(),
                    consumer: floe_inference::ModelConsumer::new(request.consumer.clone()).unwrap(),
                    capabilities: ModelCapabilities(vec![
                        floe_agent_contract::ModelCapability::Chat,
                        floe_agent_contract::ModelCapability::ToolProposals,
                    ]),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([88; 32]),
                    selection_commitment: ModelSelectionCommitment([89; 32]),
                    budget_profile: ModelBudgetProfile::unknown(),
                },
                transport: Box::new(CapacityTransport {
                    model: self.clone(),
                    consumer: request.consumer.clone(),
                }) as Box<dyn PreparedModelTransport>,
            }))
        })
    }
}
struct CapacityTransport {
    model: CapacityModel,
    consumer: String,
}
impl PreparedModelTransport for CapacityTransport {
    fn validate_request(&self, request: &InferenceRequest) -> Result<(), AgentFailure> {
        request.validate()?;
        if self.consumer == floe_conversation::COMPACTION_CONSUMER
            && self.model.failure_mode.load(Ordering::SeqCst) == 3
        {
            self.model.rejected.fetch_add(1, Ordering::SeqCst);
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        let bytes = serde_json::to_vec(&request.envelope).unwrap().len();
        if bytes > self.model.limit.load(Ordering::SeqCst) {
            self.model.rejected.fetch_add(1, Ordering::SeqCst);
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        Ok(())
    }
    fn dispatch_target(&self) -> floe_access::ModelDispatchTarget {
        floe_access::ModelDispatchTarget::Device
    }
    fn generate<'a>(
        &'a self,
        request: InferenceRequest,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'a, Result<CanonicalModelResponse, AgentFailure>> {
        Box::pin(async move {
            self.validate_request(&request)?;
            assert!(target.matches(&ModelBindingDigest([88; 32]), ProcessingBoundary::Device));
            let text = if self.consumer == floe_conversation::COMPACTION_CONSUMER {
                assert!(request.catalog.tools.is_empty() && request.catalog.cards.is_empty());
                assert!(request.envelope.contextual_data.memories.is_empty());
                "Earlier test discussion.".to_owned()
            } else {
                self.model.foreground_reply.clone()
            };
            let cancellation = request.cancellation.clone();
            self.model
                .calls
                .lock()
                .unwrap()
                .push((self.consumer.clone(), request));
            if self.consumer == floe_conversation::COMPACTION_CONSUMER {
                if let Some(barrier) = &self.model.compaction_barrier {
                    barrier.wait(&cancellation).await?;
                }
            }
            let mode = self.model.failure_mode.load(Ordering::SeqCst);
            if (mode == 1 && self.consumer == CONVERSATION_CONSUMER)
                || (mode == 2 && self.consumer == floe_conversation::COMPACTION_CONSUMER)
            {
                return Err(AgentFailure::ModelInputCapacityExceeded);
            }
            Ok(response(vec![ModelStep::Answer {
                text,
                artifacts: vec![],
            }]))
        })
    }
}

pub(crate) fn run_capacity_compaction_case() {
    let model = CapacityModel {
        limit: Arc::new(AtomicUsize::new(usize::MAX)),
        calls: Arc::new(Mutex::new(vec![])),
        rejected: Arc::new(AtomicUsize::new(0)),
        failure_mode: Arc::new(AtomicUsize::new(0)),
        foreground_reply: "A".repeat(500),
        compaction_barrier: None,
        plans: Arc::new(AtomicUsize::new(0)),
    };
    let profile = IsolatedProfile::new();
    let open = || {
        floe_app::open_default_with_options(
            profile.root.path().to_str().unwrap(),
            AppOpenOptions::default().with_model_provider_factory(model.clone()),
        )
        .unwrap()
    };
    let host = open();
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    let session = start_session(&host);
    for ordinal in 0..2 {
        let command = start_turn(
            &host,
            session,
            format!("Input {ordinal}:{}", "u".repeat(500)),
        );
        let receipt = wait_terminal_run(&host, command.run_id);
        assert_eq!(
            receipt.state,
            floe_conversation::RunState::Completed,
            "{receipt:?}"
        );
    }
    let before = read_session(&host, session);
    let prior_size = serde_json::to_vec(&model.calls.lock().unwrap().last().unwrap().1.envelope)
        .unwrap()
        .len();
    model.limit.store(prior_size + 500, Ordering::SeqCst);
    let current = format!("Current input:{}", "v".repeat(500));
    let command = start_turn(&host, session, current.clone());
    let receipt = wait_terminal_run(&host, command.run_id);
    assert_eq!(
        receipt.state,
        floe_conversation::RunState::Completed,
        "{receipt:?}"
    );
    assert_eq!(
        receipt.attempt_refs.len(),
        1,
        "the rejected foreground input was not dispatched"
    );
    let after = read_session(&host, session);
    assert_eq!(&after.messages[..before.messages.len()], &before.messages);
    assert_eq!(after.messages.len(), before.messages.len() + 2);
    let calls = model.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|(consumer, _)| consumer.as_str())
            .collect::<Vec<_>>(),
        vec![
            CONVERSATION_CONSUMER,
            CONVERSATION_CONSUMER,
            floe_conversation::COMPACTION_CONSUMER,
            CONVERSATION_CONSUMER
        ]
    );
    let last = &calls.last().unwrap().1.envelope.conversation;
    assert!(last.history.iter().any(|entry| matches!(entry, floe_agent_contract::ModelConversationEntry::Assistant { text, .. } if text == "Earlier test discussion.")));
    assert!(last.current_turn.iter().any(|entry| matches!(entry, floe_agent_contract::ModelConversationEntry::User { text, .. } if text == &current)));
    assert!(model.rejected.load(Ordering::SeqCst) > 0);
    let count = calls.len();
    drop(calls);
    drop(host);
    let reopened = open();
    assert_eq!(prepare_runtime(&reopened), RuntimeReadinessState::Ready);
    assert_eq!(read_session(&reopened, session).messages, after.messages);
    assert_eq!(
        wait_terminal_run(&reopened, command.run_id).run_id,
        receipt.run_id
    );
    assert_eq!(
        model.calls.lock().unwrap().len(),
        count,
        "reopening does not redispatch any summary or foreground answer"
    );
}

pub(crate) fn run_capacity_failure_boundaries() {
    for mode in [1, 2, 3] {
        let model = CapacityModel {
            limit: Arc::new(AtomicUsize::new(usize::MAX)),
            calls: Arc::new(Mutex::new(vec![])),
            rejected: Arc::new(AtomicUsize::new(0)),
            failure_mode: Arc::new(AtomicUsize::new(0)),
            foreground_reply: "A".repeat(500),
            compaction_barrier: None,
            plans: Arc::new(AtomicUsize::new(0)),
        };
        let profile = IsolatedProfile::new();
        let open = || {
            floe_app::open_default_with_options(
                profile.root.path().to_str().unwrap(),
                AppOpenOptions::default().with_model_provider_factory(model.clone()),
            )
            .unwrap()
        };
        let host = open();
        assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
        let session = start_session(&host);
        for ordinal in 0..2 {
            let command = start_turn(
                &host,
                session,
                format!("Input {ordinal}:{}", "u".repeat(500)),
            );
            assert_eq!(
                wait_terminal_run(&host, command.run_id).state,
                floe_conversation::RunState::Completed
            );
        }
        let before = read_session(&host, session);
        model.failure_mode.store(mode, Ordering::SeqCst);
        if mode != 1 {
            let size = serde_json::to_vec(&model.calls.lock().unwrap().last().unwrap().1.envelope)
                .unwrap()
                .len();
            model.limit.store(size + 500, Ordering::SeqCst);
        }
        let command = start_turn(&host, session, format!("New input:{}", "v".repeat(500)));
        let receipt = wait_terminal_run(&host, command.run_id);
        assert_eq!(
            receipt.issue,
            Some(if mode == 2 {
                AgentFailure::Interrupted
            } else {
                AgentFailure::ModelInputCapacityExceeded
            }),
            "mode={mode}: {receipt:?}"
        );
        assert_eq!(receipt.attempt_refs.len(), usize::from(mode == 1));
        let after = read_session(&host, session);
        assert_eq!(&after.messages[..before.messages.len()], &before.messages);
        let calls = model.calls.lock().unwrap();
        let summaries = calls
            .iter()
            .filter(|(consumer, _)| consumer == floe_conversation::COMPACTION_CONSUMER)
            .count();
        assert_eq!(
            summaries,
            usize::from(mode == 2),
            "no summary dispatch on post-handoff foreground error or pure summary refusal"
        );
        assert_eq!(calls.len(), if mode == 3 { 2 } else { 3 });
        let count = calls.len();
        drop(calls);
        drop(host);
        let reopened = open();
        assert_eq!(prepare_runtime(&reopened), RuntimeReadinessState::Ready);
        assert_eq!(read_session(&reopened, session).messages, after.messages);
        assert_eq!(
            wait_terminal_run(&reopened, command.run_id).issue,
            receipt.issue
        );
        assert_eq!(
            model.calls.lock().unwrap().len(),
            count,
            "failure recovery never redispatches"
        );
    }
}

impl CapacityModel {
    pub(crate) fn history_model(reply: &str) -> Self {
        Self {
            limit: Arc::new(AtomicUsize::new(usize::MAX)),
            calls: Arc::new(Mutex::new(vec![])),
            rejected: Arc::new(AtomicUsize::new(0)),
            failure_mode: Arc::new(AtomicUsize::new(0)),
            foreground_reply: reply.to_owned(),
            compaction_barrier: None,
            plans: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub(crate) fn open(&self, profile: &IsolatedProfile) -> AppHost<AppComposition> {
        floe_app::open_default_with_options(
            profile.root.path().to_str().unwrap(),
            AppOpenOptions::default().with_model_provider_factory(self.clone()),
        )
        .unwrap()
    }
    pub(crate) fn counts(&self) -> (usize, usize, usize) {
        let calls = self.calls.lock().unwrap();
        (
            calls
                .iter()
                .filter(|(c, _)| c == CONVERSATION_CONSUMER)
                .count(),
            calls
                .iter()
                .filter(|(c, _)| c == floe_conversation::COMPACTION_CONSUMER)
                .count(),
            self.plans.load(Ordering::SeqCst),
        )
    }
}

pub(crate) fn run_compaction_shutdown_case() {
    let (barrier, entered, _release) = GenerateBarrier::new();
    let mut model = CapacityModel::history_model(&"A".repeat(500));
    model.compaction_barrier = Some(barrier);
    let profile = IsolatedProfile::new();
    let host = model.open(&profile);
    assert_eq!(prepare_runtime(&host), RuntimeReadinessState::Ready);
    let session = start_session(&host);
    for ordinal in 0..2 {
        let command = start_turn(
            &host,
            session,
            format!("Input {ordinal}:{}", "u".repeat(500)),
        );
        assert_eq!(
            wait_terminal_run(&host, command.run_id).state,
            floe_conversation::RunState::Completed
        );
    }
    let size = serde_json::to_vec(&model.calls.lock().unwrap().last().unwrap().1.envelope)
        .unwrap()
        .len();
    model.limit.store(size + 500, Ordering::SeqCst);
    let command = start_turn(&host, session, format!("Current input:{}", "v".repeat(500)));
    entered
        .recv_timeout(Duration::from_secs(10))
        .expect("summary reached its synthetic provider");
    host.shutdown()
        .expect("cancel and drain the live summary execution");
    drop(host);
    let before = model.counts();
    assert_eq!((before.0, before.1), (2, 1));
    let reopened = model.open(&profile);
    assert_eq!(prepare_runtime(&reopened), RuntimeReadinessState::Ready);
    let receipt = wait_terminal_run(&reopened, command.run_id);
    assert_ne!(receipt.state, floe_conversation::RunState::Completed);
    assert!(
        receipt.attempt_refs.is_empty(),
        "the foreground model was never dispatched"
    );
    assert_eq!(
        model.counts(),
        before,
        "shutdown recovery cannot repeat the uncertain summary"
    );
    let messages = read_session(&reopened, session).messages;
    assert_eq!(
        messages
            .iter()
            .filter(|m| matches!(m, floe_conversation::SessionMessage::User { .. }))
            .count(),
        3
    );
}
