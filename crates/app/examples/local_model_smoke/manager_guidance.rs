use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::Mutex,
    time::Duration,
};

use chrono::Utc;
use floe_access::{
    ContextualRecipientAuthority, RecipientConsent, RecipientConsentStore, SystemConsentClock,
    grant_recipient_consent,
};
use floe_agent_contract::{
    A2A_PROTOCOL_VERSION, AGENT_SCHEMA_VERSION, AgentCard, AgentContext, AgentDefinition,
    AgentFailure, AllowedCatalog, AuthorizedModelProjection, BoxFuture, DataClass,
    DelegationExecutionContext, DelegationRequest, DependencyCoverage, InvocationKey,
    ModelCallOutcome, ModelConversation, ModelConversationEntry, ModelPlacement, ModelPort,
    ModelProjectionPort, ModelProjectionRequest, ModelRequest, ModelStep, TaskId, TaskReceipt,
    TaskSnapshot, TaskState,
};
use floe_context_contract::RecipientLineage;
use floe_conversation::{ConversationModelProjection, prompts::manager_role_spec};
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_inference::{
    CANONICAL_MODEL_CONSUMER, CANONICAL_MODEL_PURPOSE, InferenceService, ModelProvider,
    SavedServerConnection,
};
use floe_kernel::{PersonId, TraceContext};
use floe_provider_adapters::{
    control::{CurrentSavedConnectionStore, SavedConnectionAdmission},
    models::{FoundationModelProvider, RootModelProvider},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/manager-guidance/corpus.json"
));
const REPETITIONS: usize = 3;
const OUTPUT_BYTES: usize = 16384;

pub(super) enum Mode {
    Foundation,
    Server,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    note: String,
    cards: BTreeMap<String, FixtureCard>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureCard {
    id: String,
    name: String,
    description: String,
    schema_version: u32,
    version: String,
    domain_tags: Vec<String>,
    skills: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum AcceptedKind {
    Answer,
    Delegate,
    AnswerOrDelegate,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct History {
    role: String,
    text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ReviewFocus {
    UnsupportedCurrentClaim,
    RequiredObservation,
    NoSuitableExpert,
    StaleNotFresh,
    UnavailableNotEmpty,
    PartialScope,
    GuessNotObserved,
    GeneralKnowledgeNoDelegation,
    SuppliedTransformNoDelegation,
    BlockerNotObservation,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    catalog: Vec<String>,
    user: String,
    #[serde(default)]
    history: Vec<History>,
    accepted_kind: AcceptedKind,
    #[serde(default)]
    accepted_agents: Vec<String>,
    result_fixture: Option<String>,
    next_accepted_kind: Option<AcceptedKind>,
    #[serde(default)]
    accepted_next_agents: Vec<String>,
    rubric: String,
    #[serde(default, skip_serializing)]
    review_focus: Vec<ReviewFocus>,
}

fn bounded(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit
}

fn load_corpus() -> Result<Corpus, AgentFailure> {
    let corpus: Corpus = serde_json::from_str(CORPUS).map_err(|_| AgentFailure::InvalidInput)?;
    validate_corpus(&corpus)?;
    Ok(corpus)
}

fn validate_corpus(corpus: &Corpus) -> Result<(), AgentFailure> {
    let mut ids = HashSet::new();
    if corpus.cases.len() != 22
        || corpus.cards.len() != 5
        || corpus
            .cases
            .iter()
            .filter(|case| case.result_fixture.is_some())
            .count()
            != 5
        || !bounded(&corpus.note, 4096)
    {
        return Err(AgentFailure::InvalidInput);
    }
    for (alias, card) in &corpus.cards {
        if !bounded(alias, 128)
            || !ids.insert(&card.id)
            || card.schema_version != AGENT_SCHEMA_VERSION
            || card.version != "1.0.0"
            || !card.domain_tags.is_empty()
            || !card.skills.is_empty()
        {
            return Err(AgentFailure::InvalidInput);
        }
        fixture_card(card).validate()?;
    }
    let mut case_ids = HashSet::new();
    for case in &corpus.cases {
        if !bounded(&case.id, 128)
            || !case_ids.insert(&case.id)
            || !bounded(&case.user, OUTPUT_BYTES)
            || !bounded(&case.rubric, 4096)
            || case.history.len() > 16
            || case.review_focus.len() > 4
            || case.review_focus.iter().collect::<HashSet<_>>().len() != case.review_focus.len()
            || case.history.iter().any(|entry| {
                !matches!(entry.role.as_str(), "user" | "assistant")
                    || !bounded(&entry.text, OUTPUT_BYTES)
            })
            || case
                .result_fixture
                .as_deref()
                .is_some_and(|text| !bounded(text, OUTPUT_BYTES))
            || case.result_fixture.is_some() != case.next_accepted_kind.is_some()
        {
            return Err(AgentFailure::InvalidInput);
        }
        let catalog = build_catalog(corpus, case)?;
        for (kind, agents) in [
            (Some(case.accepted_kind), &case.accepted_agents),
            (case.next_accepted_kind, &case.accepted_next_agents),
        ] {
            let requires_agents = matches!(
                kind,
                Some(AcceptedKind::Delegate | AcceptedKind::AnswerOrDelegate)
            );
            let mut unique = HashSet::new();
            if requires_agents == agents.is_empty()
                || agents.iter().any(|agent| {
                    !unique.insert(agent)
                        || !catalog
                            .cards
                            .iter()
                            .any(|definition| &definition.card.id == agent)
                })
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if case.result_fixture.is_some() && !matches!(case.accepted_kind, AcceptedKind::Delegate) {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(())
}

fn fixture_card(card: &FixtureCard) -> AgentCard {
    AgentCard {
        schema_version: AGENT_SCHEMA_VERSION,
        protocol_version: A2A_PROTOCOL_VERSION.into(),
        id: card.id.clone(),
        version: card.version.clone(),
        name: card.name.clone(),
        description: card.description.clone(),
        domain_tags: vec![],
        skills: vec![],
        supported_placements: vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote],
    }
}

fn build_catalog(corpus: &Corpus, case: &Case) -> Result<AllowedCatalog, AgentFailure> {
    let mut aliases = HashSet::new();
    let cards = case
        .catalog
        .iter()
        .map(|alias| {
            if !aliases.insert(alias) {
                return Err(AgentFailure::InvalidInput);
            }
            let card = corpus.cards.get(alias).ok_or(AgentFailure::InvalidInput)?;
            let definition = AgentDefinition {
                card: fixture_card(card),
                definition_revision: 1,
            };
            definition.validate()?;
            Ok(definition)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AllowedCatalog {
        cards,
        tools: vec![],
        revision: 1,
    })
}

struct SyntheticEvidence {
    session_id: Uuid,
    history_ids: HashSet<Uuid>,
}

impl floe_context::EvidenceReader for SyntheticEvidence {
    fn read_turn_coverage(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> impl std::future::Future<Output = Result<DependencyCoverage, AgentFailure>> + Send {
        let known = session_id == self.session_id && self.history_ids.contains(&turn_id);
        async move {
            Ok(if known {
                DependencyCoverage::Independent
            } else {
                DependencyCoverage::Unknown
            })
        }
    }
}

fn context() -> AgentContext {
    AgentContext {
        projection_version: 1,
        persona: None,
        memories: vec![],
        optional_context_issues: vec![],
        evidence: vec![],
    }
}

fn conversation(case: &Case) -> ModelConversation {
    ModelConversation {
        history: case
            .history
            .iter()
            .map(|entry| {
                let message_id = Uuid::new_v4();
                if entry.role == "user" {
                    ModelConversationEntry::User {
                        message_id,
                        text: entry.text.clone(),
                    }
                } else {
                    ModelConversationEntry::Assistant {
                        message_id,
                        text: entry.text.clone(),
                    }
                }
            })
            .collect(),
        current_turn: vec![ModelConversationEntry::User {
            message_id: Uuid::new_v4(),
            text: case.user.clone(),
        }],
    }
}

async fn project_case(
    catalog: &AllowedCatalog,
    conversation: ModelConversation,
    session_id: Uuid,
    principal: &str,
    scope: &ExecutionScope,
) -> Result<AuthorizedModelProjection, AgentFailure> {
    let history_ids = conversation
        .history
        .iter()
        .filter_map(|entry| match entry {
            ModelConversationEntry::User { message_id, .. }
            | ModelConversationEntry::Assistant { message_id, .. } => Some(*message_id),
            _ => None,
        })
        .collect();
    let projector = ConversationModelProjection::new(
        SyntheticEvidence {
            session_id,
            history_ids,
        },
        super::learner::SmokeResolver,
        session_id,
        context(),
        vec![DataClass::Synthetic],
        floe_experts::RunExpertEnvironmentIdentity {
            revision: catalog.revision,
            digest: [1; 32],
        },
    )?;
    let projection = projector
        .project(
            ModelProjectionRequest {
                principal: principal.into(),
                role: manager_role_spec(),
                conversation,
                catalog: catalog.clone(),
                max_output_bytes: OUTPUT_BYTES,
                correction: None,
            },
            scope,
        )
        .await?;
    if projection.coverage != DependencyCoverage::Independent
        || projection.input_data_classes != [DataClass::Synthetic]
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(projection)
}

fn first_step(steps: &[ModelStep]) -> Option<&ModelStep> {
    steps
        .iter()
        .find(|step| !matches!(step, ModelStep::Preamble { .. }))
}

fn classify_batch(steps: &[ModelStep], kind: AcceptedKind, agents: &[String]) -> bool {
    match first_step(steps) {
        Some(ModelStep::Answer { text, .. }) => {
            !matches!(kind, AcceptedKind::Delegate)
                && !text.trim().is_empty()
                && steps.iter().all(|step| {
                    matches!(step, ModelStep::Preamble { .. } | ModelStep::Answer { .. })
                })
        }
        Some(ModelStep::Delegate { .. }) => {
            !matches!(kind, AcceptedKind::Answer)
                && steps.iter().all(|step| match step {
                    ModelStep::Preamble { .. } => true,
                    ModelStep::Delegate {
                        agent_id,
                        definition_revision,
                        message,
                        context_refs,
                    } => {
                        agents.contains(agent_id)
                            && *definition_revision == 1
                            && bounded(message, OUTPUT_BYTES)
                            && floe_agent_contract::valid_context_refs(context_refs)
                    }
                    _ => false,
                })
        }
        _ => false,
    }
}

fn settled_fixture_exchange(
    step: &ModelStep,
    result: &str,
    principal: &str,
    session_id: Uuid,
    run_id: Uuid,
) -> Result<ModelConversationEntry, AgentFailure> {
    let ModelStep::Delegate {
        agent_id,
        definition_revision,
        message,
        context_refs,
    } = step
    else {
        return Err(AgentFailure::InvalidInput);
    };
    let task_id = TaskId::new();
    let request = DelegationRequest {
        task_id,
        parent_run_id: Some(run_id),
        principal: principal.into(),
        invocation_key: InvocationKey::new(),
        selected_agent_id: agent_id.clone(),
        selected_definition_revision: *definition_revision,
        message: message.clone(),
        context_refs: context_refs.clone(),
        execution_context: DelegationExecutionContext {
            session_id,
            device_id: "synthetic-manager-eval".into(),
            agent_context: context(),
            max_output_bytes: OUTPUT_BYTES,
        },
    };
    let receipt = TaskReceipt {
        task_id,
        snapshot: TaskSnapshot {
            task_id,
            parent_run_id: request.parent_run_id,
            principal: request.principal.clone(),
            agent_id: agent_id.clone(),
            definition_revision: *definition_revision,
            state: TaskState::Completed,
            result: Some(result.into()),
            artifacts: vec![],
            coverage: DependencyCoverage::Independent,
            issue: None,
        },
        replay: None,
    };
    let exchange = ModelConversationEntry::DelegationExchange { request, receipt };
    exchange.validate()?;
    Ok(exchange)
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

struct ReportMetadata {
    stage: String,
    commit: String,
    profile: &'static str,
    model_id: Option<String>,
    model_id_origin: &'static str,
    configuration_hash: String,
}

fn report_case(
    case: &Case,
    repetition: usize,
    metadata: &ReportMetadata,
    projection: &AuthorizedModelProjection,
    _catalog: &AllowedCatalog,
    steps: &[ModelStep],
    follow_up: bool,
    failure: Option<AgentFailure>,
) -> Value {
    let kind = if follow_up {
        case.next_accepted_kind.unwrap_or(AcceptedKind::Answer)
    } else {
        case.accepted_kind
    };
    let agents = if follow_up {
        &case.accepted_next_agents
    } else {
        &case.accepted_agents
    };
    let accepted = failure.is_none() && classify_batch(steps, kind, agents);
    json!({
        "schema_version": 1, "case_id": case.id, "repetition": repetition, "stage": metadata.stage, "commit_sha": metadata.commit,
        "corpus_sha256": digest(CORPUS), "provider": if metadata.profile == "foundation-device" { "foundation" } else { "server" },
        "profile": metadata.profile, "model_id": metadata.model_id, "model_id_origin": metadata.model_id_origin,
        "configuration_sha256": metadata.configuration_hash,
        "prompt_components": projection.envelope.manifest.prompt_components,
        "stable_instructions_sha256": projection.envelope.manifest.stable_prompt_sha256,
        "run_frame_sha256": projection.envelope.manifest.run_frame_sha256,
        "expert_environment": projection.envelope.manifest.expert_environment,
        "ordered_cards": projection.envelope.manifest.agent_cards,
        "phase": if follow_up { "synthesis" } else { "selection" }, "batch_steps": steps, "choice_accepted": accepted,
        "status": if failure.is_some() { "EXECUTION_FAILURE" } else if accepted { "REVIEW_REQUIRED" } else { "BEHAVIOR_FAILURE" },
        "failure": failure, "behavior_review": "pending", "rubric": case.rubric, "personal_data": false,
        "accepted_kind": kind, "accepted_agents": agents, "review_focus": case.review_focus,
        "synthetic_result_replay": follow_up, "real_expert_execution": false,
    })
}

fn report_summary(corpus: &Corpus, metadata: &ReportMetadata, all_accepted: bool) -> Value {
    let synthesis_cases = corpus
        .cases
        .iter()
        .filter(|case| case.result_fixture.is_some())
        .count();
    json!({
        "schema_version": 1, "status": if all_accepted { "REVIEW_REQUIRED" } else { "FAIL" },
        "commit_sha": metadata.commit, "corpus_sha256": digest(CORPUS), "stage": metadata.stage,
        "provider": if metadata.profile == "foundation-device" { "foundation" } else { "server" },
        "profile": metadata.profile, "model_id": metadata.model_id, "model_id_origin": metadata.model_id_origin,
        "configuration_sha256": metadata.configuration_hash,
        "cases": corpus.cases.len(), "synthesis_cases": synthesis_cases, "repetitions": REPETITIONS,
        "expected_report_records": REPETITIONS * (corpus.cases.len() + synthesis_cases),
        "behavior_review": "pending", "personal_data": false,
    })
}

async fn run_case(
    model: &impl ModelPort,
    corpus: &Corpus,
    case: &Case,
    repetition: usize,
    metadata: &ReportMetadata,
    principal: &str,
    session_id: Uuid,
    lineage: RecipientLineage,
) -> Result<bool, AgentFailure> {
    let catalog = build_catalog(corpus, case)?;
    let mut conversation = conversation(case);
    let mut accepted = true;
    for follow_up in [false, true] {
        let ledger = BudgetLedger::new(BudgetConfig::new(8192, 1_000_000), ModelUsage::default());
        let scope = ExecutionScope::root(
            Cancellation::default(),
            tokio::time::Instant::now() + Duration::from_secs(40),
            ledger.work_lease(),
            TraceContext::new(session_id),
        );
        let projection = project_case(
            &catalog,
            conversation.clone(),
            session_id,
            principal,
            &scope,
        )
        .await?;
        let request = ModelRequest {
            attempt_id: Uuid::new_v4(),
            principal: principal.into(),
            projection: projection.clone(),
            catalog: catalog.clone(),
            purpose: CANONICAL_MODEL_PURPOSE.into(),
            consumer: CANONICAL_MODEL_CONSUMER.into(),
            preferred_profile_id: Some(metadata.profile.into()),
            replay: vec![],
            lineage: Some(lineage),
        };
        let (steps, failure) = match model.generate(request, &scope).await {
            Ok(ModelCallOutcome::Ready(response)) => (response.steps, None),
            Ok(ModelCallOutcome::NeedsUserAction(_)) => (vec![], Some(AgentFailure::PolicyDenied)),
            Err(failure) => (vec![], Some(failure)),
        };
        let report = report_case(
            case,
            repetition,
            metadata,
            &projection,
            &catalog,
            &steps,
            follow_up,
            failure,
        );
        let shape_ok = report["choice_accepted"] == true;
        println!("{report}");
        accepted &= shape_ok;
        if follow_up || !shape_ok {
            break;
        }
        let Some(result) = &case.result_fixture else {
            break;
        };
        for step in &steps {
            match step {
                ModelStep::Preamble { text } => {
                    conversation
                        .current_turn
                        .push(ModelConversationEntry::Preamble {
                            message_id: Uuid::new_v4(),
                            text: text.clone(),
                        })
                }
                ModelStep::Delegate { .. } => {
                    conversation.current_turn.push(settled_fixture_exchange(
                        step,
                        result,
                        principal,
                        session_id,
                        lineage.origin_run_id(),
                    )?)
                }
                _ => return Err(AgentFailure::InvalidModelOutput),
            }
        }
    }
    Ok(accepted)
}

#[derive(Default)]
struct MemoryConsents(Mutex<HashMap<Uuid, RecipientConsent>>);

impl RecipientConsentStore for MemoryConsents {
    fn grant_consent<'a>(
        &'a self,
        consent: RecipientConsent,
    ) -> BoxFuture<'a, Result<RecipientConsent, AgentFailure>> {
        Box::pin(async move {
            consent.validate().map_err(|_| AgentFailure::InvalidInput)?;
            self.0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .entry(consent.id())
                .or_insert_with(|| consent.clone());
            Ok(consent)
        })
    }
    fn find_consent<'a>(
        &'a self,
        consent_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<RecipientConsent>, AgentFailure>> {
        Box::pin(async move {
            Ok(self
                .0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .get(&consent_id)
                .cloned())
        })
    }
    fn revoke_consent<'a>(&'a self, consent_id: Uuid) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            let mut records = self
                .0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let consent = records.get_mut(&consent_id).ok_or(AgentFailure::NotFound)?;
            *consent = consent
                .clone()
                .revoked()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            Ok(())
        })
    }
    fn prune_expired<'a>(&'a self, now_unix_ms: i64) -> BoxFuture<'a, Result<u64, AgentFailure>> {
        Box::pin(async move {
            let mut records = self
                .0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let before = records.len();
            records.retain(|_, consent| consent.expires_at().timestamp_millis() > now_unix_ms);
            Ok((before - records.len()) as u64)
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectionFile {
    base_url: String,
    token: String,
    client_id: String,
    person_id: String,
    device_id: String,
}

fn read_connection(path: &Path) -> Result<SavedServerConnection, AgentFailure> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let metadata = file.metadata().map_err(|_| AgentFailure::PolicyDenied)?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || metadata.len() > 16384
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file.take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if bytes.len() > 16384 {
        return Err(AgentFailure::PolicyDenied);
    }
    let saved: ConnectionFile =
        serde_json::from_slice(&bytes).map_err(|_| AgentFailure::InvalidInput)?;
    Ok(SavedServerConnection {
        base_url: saved.base_url,
        token: saved.token,
        client_id: saved.client_id,
        person_id: saved.person_id,
        device_id: saved.device_id,
    })
}

fn unverified(reason: &str) -> std::process::ExitCode {
    println!(
        "{}",
        json!({"schema_version":1,"status":"UNVERIFIED","reason":reason,"personal_data":false})
    );
    std::process::ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        os::unix::fs::{PermissionsExt, symlink},
    };

    fn case<'corpus>(corpus: &'corpus Corpus, id: &str) -> &'corpus Case {
        corpus.cases.iter().find(|case| case.id == id).unwrap()
    }

    const FIRST_18_SHA256: &str =
        "13de056b5ee1c2d1d5f7fc693248d59bf8bbf1489f0152b084f74e12a9bc59ef";

    #[test]
    fn frozen_cases_and_typed_review_focus_match_the_checkpoint() {
        let corpus = load_corpus().unwrap();
        assert_eq!(
            digest(serde_json::to_vec(&corpus.cases[..18]).unwrap()),
            FIRST_18_SHA256
        );
        let expected = [
            ("D01", json!(["general_knowledge_no_delegation"])),
            ("D02", json!(["supplied_transform_no_delegation"])),
            ("D03", json!([])),
            (
                "R01",
                json!(["required_observation", "unsupported_current_claim"]),
            ),
            (
                "R02",
                json!(["required_observation", "unsupported_current_claim"]),
            ),
            (
                "R03",
                json!(["required_observation", "unsupported_current_claim"]),
            ),
            (
                "R04",
                json!(["required_observation", "unsupported_current_claim"]),
            ),
            (
                "L01",
                json!(["no_suitable_expert", "unsupported_current_claim"]),
            ),
            (
                "L02",
                json!(["no_suitable_expert", "unsupported_current_claim"]),
            ),
            ("U01", json!(["unsupported_current_claim"])),
            ("U02", json!(["unsupported_current_claim"])),
            ("U03", json!(["supplied_transform_no_delegation"])),
            ("J01", json!([])),
            (
                "F01",
                json!([
                    "stale_not_fresh",
                    "required_observation",
                    "unsupported_current_claim"
                ]),
            ),
            ("S01", json!([])),
            (
                "S02",
                json!(["unavailable_not_empty", "unsupported_current_claim"]),
            ),
            ("S03", json!(["partial_scope"])),
            (
                "S04",
                json!(["blocker_not_observation", "unsupported_current_claim"]),
            ),
            (
                "F02",
                json!([
                    "stale_not_fresh",
                    "required_observation",
                    "unsupported_current_claim"
                ]),
            ),
            (
                "G01",
                json!(["guess_not_observed", "unsupported_current_claim"]),
            ),
            (
                "G02",
                json!(["guess_not_observed", "unsupported_current_claim"]),
            ),
            (
                "S05",
                json!(["unavailable_not_empty", "unsupported_current_claim"]),
            ),
        ];
        assert_eq!(
            corpus
                .cases
                .iter()
                .map(|case| case.id.as_str())
                .collect::<Vec<_>>(),
            expected.iter().map(|(id, _)| *id).collect::<Vec<_>>()
        );
        for (id, focus) in expected {
            assert_eq!(
                serde_json::to_value(&case(&corpus, id).review_focus).unwrap(),
                focus
            );
        }
        assert_eq!(
            corpus
                .cases
                .iter()
                .filter(|case| case.result_fixture.is_some())
                .map(|case| case.id.as_str())
                .collect::<Vec<_>>(),
            ["S01", "S02", "S03", "S04", "S05"]
        );
        for focus in [
            json!(["not_a_review_focus"]),
            json!(["partial_scope", "partial_scope"]),
            json!([
                "partial_scope",
                "unsupported_current_claim",
                "required_observation",
                "stale_not_fresh",
                "guess_not_observed"
            ]),
        ] {
            let mut value: Value = serde_json::from_str(CORPUS).unwrap();
            value["cases"][0]["review_focus"] = focus;
            assert!(match serde_json::from_value::<Corpus>(value) {
                Ok(corpus) => validate_corpus(&corpus).is_err(),
                Err(_) => true,
            });
        }
        for mutation in 0..3 {
            let mut corpus = load_corpus().unwrap();
            match mutation {
                0 => {
                    corpus.cases.pop();
                }
                1 => {
                    corpus.cards.remove("note");
                }
                _ => {
                    corpus.cases[21].result_fixture = None;
                    corpus.cases[21].next_accepted_kind = None;
                }
            }
            assert!(validate_corpus(&corpus).is_err());
        }
    }

    #[tokio::test]
    async fn review_metadata_never_changes_projection_or_shape_acceptance() {
        let corpus = load_corpus().unwrap();
        let session_id = Uuid::new_v4();
        for case in &corpus.cases {
            let mut without_focus = case.clone();
            without_focus.review_focus.clear();
            let catalog = build_catalog(&corpus, case).unwrap();
            let plain_catalog = build_catalog(&corpus, &without_focus).unwrap();
            assert_eq!(catalog, plain_catalog);
            assert_eq!(
                serde_json::to_vec(case).unwrap(),
                serde_json::to_vec(&without_focus).unwrap()
            );
            let input = conversation(case);
            let original = project_case(
                &catalog,
                input.clone(),
                session_id,
                "synthetic-person",
                &scope(),
            )
            .await
            .unwrap();
            let plain = project_case(
                &plain_catalog,
                input,
                session_id,
                "synthetic-person",
                &scope(),
            )
            .await
            .unwrap();
            assert_eq!(original.envelope, plain.envelope);
            assert_eq!(original.projection_revision, plain.projection_revision);
            assert_eq!(original.coverage, plain.coverage);
            assert_eq!(original.input_data_classes, plain.input_data_classes);
            for steps in [
                vec![answer()],
                vec![delegate("example.eval.iris")],
                vec![delegate("absent")],
            ] {
                assert_eq!(
                    classify_batch(&steps, case.accepted_kind, &case.accepted_agents),
                    classify_batch(
                        &steps,
                        without_focus.accepted_kind,
                        &without_focus.accepted_agents
                    )
                );
            }
        }
    }

    #[test]
    fn new_cases_preserve_exact_shapes_without_grading_truth() {
        let corpus = load_corpus().unwrap();
        let stale = case(&corpus, "F02");
        assert!(classify_batch(
            &[delegate("example.eval.iris")],
            stale.accepted_kind,
            &stale.accepted_agents
        ));
        for steps in [
            vec![answer()],
            vec![delegate("example.eval.mica")],
            vec![delegate("example.eval.cedar")],
        ] {
            assert!(!classify_batch(
                &steps,
                stale.accepted_kind,
                &stale.accepted_agents
            ));
        }
        for id in ["G01", "G02"] {
            let guess = case(&corpus, id);
            let fabricated = ModelStep::Answer {
                text: "Q-99 is blocked right now.".into(),
                artifacts: vec![],
            };
            assert!(classify_batch(
                &[fabricated],
                guess.accepted_kind,
                &guess.accepted_agents
            ));
            assert!(guess.review_focus.contains(&ReviewFocus::GuessNotObserved));
        }
        let korean = case(&corpus, "S05");
        let english = case(&corpus, "S02");
        assert_eq!(korean.accepted_kind, english.accepted_kind);
        assert_eq!(korean.accepted_agents, english.accepted_agents);
        assert_eq!(korean.result_fixture, english.result_fixture);
        assert_eq!(korean.next_accepted_kind, english.next_accepted_kind);
        assert_eq!(korean.accepted_next_agents, english.accepted_next_agents);
    }

    fn answer() -> ModelStep {
        ModelStep::Answer {
            text: "synthetic answer".into(),
            artifacts: vec![],
        }
    }

    fn delegate(agent_id: &str) -> ModelStep {
        ModelStep::Delegate {
            agent_id: agent_id.into(),
            definition_revision: 1,
            message: "Observe the selected queue for this request.".into(),
            context_refs: vec![],
        }
    }

    fn scope() -> ExecutionScope {
        let ledger = BudgetLedger::new(BudgetConfig::new(8192, 1_000_000), ModelUsage::default());
        ExecutionScope::root(
            Cancellation::default(),
            tokio::time::Instant::now() + Duration::from_secs(5),
            ledger.work_lease(),
            TraceContext::new(Uuid::new_v4()),
        )
    }

    fn metadata() -> ReportMetadata {
        ReportMetadata {
            stage: "baseline".into(),
            commit: "test-commit".into(),
            profile: "server-model",
            model_id: Some("operator-model".into()),
            model_id_origin: "operator_configuration",
            configuration_hash: "configuration-hash".into(),
        }
    }

    #[test]
    fn fixed_corpus_and_discovery_variants_are_consistent() {
        let corpus = load_corpus().unwrap();
        assert_eq!(corpus.cases.len(), 22);
        assert_eq!(corpus.cards.len(), 5);
        let original = build_catalog(&corpus, case(&corpus, "R01")).unwrap();
        let reversed = build_catalog(&corpus, case(&corpus, "R03")).unwrap();
        assert_eq!(
            original.cards.iter().rev().collect::<Vec<_>>(),
            reversed.cards.iter().collect::<Vec<_>>()
        );
        let renamed = build_catalog(&corpus, case(&corpus, "R04")).unwrap();
        assert_eq!(
            original.cards[1].card.description,
            renamed.cards[1].card.description
        );
        assert_ne!(original.cards[1].card.id, renamed.cards[1].card.id);
        assert_ne!(original.cards[1].card.name, renamed.cards[1].card.name);
        for case in &corpus.cases {
            let catalog = build_catalog(&corpus, case).unwrap();
            assert!(catalog.tools.is_empty());
            for definition in catalog.cards {
                definition.validate().unwrap();
                assert_eq!(definition.definition_revision, 1);
                assert_eq!(
                    definition.card.supported_placements,
                    [ModelPlacement::DeviceLocal, ModelPlacement::Remote]
                );
            }
        }
    }

    #[test]
    fn malformed_fixture_aliases_ids_and_limits_fail_closed() {
        for mutation in 0..8 {
            let mut corpus = load_corpus().unwrap();
            match mutation {
                0 => corpus.cases[0].catalog.push("unknown".into()),
                1 => corpus.cases[3].accepted_agents = vec!["absent".into()],
                2 => corpus.cases[0].user = "x".repeat(OUTPUT_BYTES + 1),
                3 => corpus.cases[1].id = corpus.cases[0].id.clone(),
                4 => corpus.cards.get_mut("iris").unwrap().id = corpus.cards["mica"].id.clone(),
                5 => corpus.cases[14].next_accepted_kind = None,
                6 => {
                    let alias = corpus.cases[0].catalog[0].clone();
                    corpus.cases[0].catalog.push(alias);
                }
                _ => corpus.cases[3].accepted_agents.clear(),
            }
            assert!(validate_corpus(&corpus).is_err());
        }
    }

    #[tokio::test]
    async fn every_case_projects_canonical_synthetic_context_without_source_io() {
        let corpus = load_corpus().unwrap();
        for case in &corpus.cases {
            let catalog = build_catalog(&corpus, case).unwrap();
            let conversation = conversation(case);
            let projection = project_case(
                &catalog,
                conversation.clone(),
                Uuid::new_v4(),
                "synthetic-person",
                &scope(),
            )
            .await
            .unwrap();
            assert_eq!(projection.envelope.conversation, conversation);
            assert_eq!(projection.input_data_classes, [DataClass::Synthetic]);
            assert_eq!(projection.coverage, DependencyCoverage::Independent);
            assert_eq!(
                projection.envelope.stable_instructions,
                floe_conversation::prompts::manager_prompt(None).unwrap()
            );
            assert_eq!(
                projection.envelope.run_instructions.response_contract,
                floe_conversation::MANAGER_OUTPUT_CONTRACT
            );
        }
        let evidence = SyntheticEvidence {
            session_id: Uuid::new_v4(),
            history_ids: HashSet::new(),
        };
        use floe_context::EvidenceReader;
        assert_eq!(
            evidence
                .read_turn_coverage(evidence.session_id, Uuid::new_v4())
                .await
                .unwrap(),
            DependencyCoverage::Unknown
        );
    }

    #[test]
    fn result_exchanges_preserve_identity_and_validate_the_real_contract() {
        let corpus = load_corpus().unwrap();
        for case in corpus
            .cases
            .iter()
            .filter(|case| case.result_fixture.is_some())
        {
            let step = delegate(&case.accepted_agents[0]);
            let exchange = settled_fixture_exchange(
                &step,
                case.result_fixture.as_deref().unwrap(),
                "synthetic-person",
                Uuid::new_v4(),
                Uuid::new_v4(),
            )
            .unwrap();
            exchange.validate().unwrap();
            let ModelConversationEntry::DelegationExchange { request, receipt } = &exchange else {
                unreachable!()
            };
            assert_eq!(request.selected_agent_id, case.accepted_agents[0]);
            assert_eq!(request.selected_definition_revision, 1);
            assert_eq!(
                request.message,
                "Observe the selected queue for this request."
            );
            assert_eq!(receipt.snapshot.result, case.result_fixture);
            assert_eq!(receipt.snapshot.coverage, DependencyCoverage::Independent);
            assert!(receipt.replay.is_none());
            let mut corrupted = exchange.clone();
            if let ModelConversationEntry::DelegationExchange { receipt, .. } = &mut corrupted {
                receipt.snapshot.agent_id = "wrong-agent".into();
            }
            assert!(corrupted.validate().is_err());
        }
    }

    #[test]
    fn classification_checks_entire_batch_without_judging_answer_truth() {
        let agents: Vec<String> = vec!["example.eval.iris".into()];
        assert!(classify_batch(
            &[
                ModelStep::Preamble {
                    text: "preamble".into()
                },
                delegate(&agents[0])
            ],
            AcceptedKind::Delegate,
            &agents
        ));
        assert!(!classify_batch(
            &[delegate(&agents[0]), delegate("absent")],
            AcceptedKind::Delegate,
            &agents
        ));
        assert!(!classify_batch(
            &[delegate(&agents[0]), answer()],
            AcceptedKind::Delegate,
            &agents
        ));
        assert!(!classify_batch(
            &[ModelStep::Preamble {
                text: "preamble".into()
            }],
            AcceptedKind::Answer,
            &[]
        ));
        assert!(classify_batch(
            &[answer()],
            AcceptedKind::AnswerOrDelegate,
            &agents
        ));
        assert!(classify_batch(
            &[delegate(&agents[0])],
            AcceptedKind::AnswerOrDelegate,
            &agents
        ));
        assert!(!classify_batch(
            &[delegate(&agents[0])],
            AcceptedKind::Answer,
            &[]
        ));
    }

    struct RecordedModel {
        requests: Mutex<Vec<ModelRequest>>,
        batches: Mutex<VecDeque<Vec<ModelStep>>>,
    }

    impl ModelPort for RecordedModel {
        fn generate<'model>(
            &'model self,
            request: ModelRequest,
            _: &'model ExecutionScope,
        ) -> BoxFuture<'model, Result<ModelCallOutcome, AgentFailure>> {
            Box::pin(async move {
                let attempt_id = request.attempt_id;
                self.requests.lock().unwrap().push(request);
                let steps = self.batches.lock().unwrap().pop_front().unwrap();
                Ok(ModelCallOutcome::Ready(
                    floe_agent_contract::ModelResponse {
                        attempt_id,
                        steps,
                        usage: floe_agent_contract::ModelUsage::default(),
                    },
                ))
            })
        }
    }

    #[tokio::test]
    async fn runner_calls_once_or_twice_and_never_executes_experts() {
        let corpus = load_corpus().unwrap();
        let mut calls = 0;
        for case in &corpus.cases {
            for repetition in 1..=REPETITIONS {
                let batches = if case.result_fixture.is_some() {
                    vec![
                        vec![delegate(&case.accepted_agents[0])],
                        vec![if case.id == "S03" {
                            delegate(&case.accepted_agents[0])
                        } else {
                            answer()
                        }],
                    ]
                } else {
                    vec![vec![
                        if matches!(case.accepted_kind, AcceptedKind::Delegate) {
                            delegate(&case.accepted_agents[0])
                        } else {
                            answer()
                        },
                    ]]
                };
                let model = RecordedModel {
                    requests: Mutex::new(vec![]),
                    batches: Mutex::new(batches.into()),
                };
                let session_id = Uuid::new_v4();
                let lineage = RecipientLineage::try_new(session_id, Uuid::new_v4()).unwrap();
                assert!(
                    run_case(
                        &model,
                        &corpus,
                        case,
                        repetition,
                        &metadata(),
                        "synthetic-person",
                        session_id,
                        lineage
                    )
                    .await
                    .unwrap()
                );
                let requests = model.requests.lock().unwrap();
                assert_eq!(
                    requests.len(),
                    if case.result_fixture.is_some() { 2 } else { 1 }
                );
                calls += requests.len();
                for request in requests.iter() {
                    assert_eq!(
                        request.preferred_profile_id.as_deref(),
                        Some("server-model")
                    );
                    assert!(request.replay.is_empty());
                    assert_eq!(request.lineage, Some(lineage));
                }
                if requests.len() == 2 {
                    assert!(
                        requests[1]
                            .projection
                            .envelope
                            .conversation
                            .current_turn
                            .iter()
                            .any(|entry| matches!(
                                entry,
                                ModelConversationEntry::DelegationExchange { .. }
                            ))
                    );
                }
            }
        }
        assert_eq!(calls, 81);
    }

    #[tokio::test]
    async fn reports_preserve_batches_and_require_semantic_review_without_secrets() {
        let corpus = load_corpus().unwrap();
        let case = case(&corpus, "S02");
        let catalog = build_catalog(&corpus, case).unwrap();
        let projection = project_case(
            &catalog,
            conversation(case),
            Uuid::new_v4(),
            "synthetic-person",
            &scope(),
        )
        .await
        .unwrap();
        let steps = vec![
            ModelStep::Preamble {
                text: "preamble".into(),
            },
            answer(),
        ];
        let report = report_case(
            case,
            1,
            &metadata(),
            &projection,
            &catalog,
            &steps,
            true,
            None,
        );
        assert_eq!(report["batch_steps"], serde_json::to_value(&steps).unwrap());
        assert_eq!(report["status"], "REVIEW_REQUIRED");
        assert_eq!(report["behavior_review"], "pending");
        assert_eq!(report["rubric"], case.rubric);
        assert_eq!(report["model_id_origin"], "operator_configuration");
        assert_eq!(report["accepted_kind"], "answer");
        assert_eq!(report["accepted_agents"], json!([]));
        assert_eq!(
            report["review_focus"],
            json!(["unavailable_not_empty", "unsupported_current_claim"])
        );
        assert_eq!(
            report["prompt_components"],
            serde_json::to_value(&projection.envelope.manifest.prompt_components).unwrap()
        );
        assert_eq!(
            report["stable_instructions_sha256"],
            projection.envelope.manifest.stable_prompt_sha256
        );
        assert_eq!(
            report["run_frame_sha256"],
            projection.envelope.manifest.run_frame_sha256
        );
        let summary = report_summary(&corpus, &metadata(), true);
        assert_eq!(summary["cases"], 22);
        assert_eq!(summary["synthesis_cases"], 5);
        assert_eq!(summary["repetitions"], 3);
        assert_eq!(summary["expected_report_records"], 81);
        assert_eq!(summary["status"], "REVIEW_REQUIRED");
        assert_eq!(summary["behavior_review"], "pending");
        assert_eq!(
            report_summary(&corpus, &metadata(), false)["status"],
            "FAIL"
        );
        for field in [
            "commit_sha",
            "corpus_sha256",
            "stage",
            "provider",
            "profile",
            "model_id",
            "model_id_origin",
            "configuration_sha256",
            "personal_data",
        ] {
            assert_eq!(summary[field], report[field]);
        }
        let encoded = report.to_string();
        for field in [
            "token",
            "endpoint",
            "base_url",
            "bearer",
            "receipt",
            "replay",
            "client_id",
            "person_id",
            "device_id",
        ] {
            assert!(!encoded.contains(&format!("\"{field}\"")));
            assert!(!summary.to_string().contains(&format!("\"{field}\"")));
        }
        let failed = report_case(
            case,
            1,
            &metadata(),
            &projection,
            &catalog,
            &[delegate("absent")],
            false,
            None,
        );
        assert_eq!(failed["status"], "BEHAVIOR_FAILURE");
        assert_eq!(failed["accepted_kind"], "delegate");
        assert_eq!(failed["accepted_agents"], json!(["example.eval.iris"]));
        let rejected = report_case(
            case,
            1,
            &metadata(),
            &projection,
            &catalog,
            &[],
            false,
            Some(AgentFailure::InvalidModelOutput),
        );
        assert_eq!(rejected["status"], "EXECUTION_FAILURE");
    }

    #[test]
    fn connection_file_requires_regular_owner_only_non_symlink_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("connection.json");
        let saved = json!({"base_url":"http://127.0.0.1:1","token":"test-secret","client_id":"test-client","person_id":PersonId::new().to_string(),"device_id":"test-device"});
        std::fs::write(&path, saved.to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_connection(&path).unwrap().token, "test-secret");
        let link = root.path().join("symlink.json");
        symlink(&path, &link).unwrap();
        assert!(read_connection(&link).is_err());
        assert!(read_connection(root.path()).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_connection(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&path, "not JSON test-secret").unwrap();
        assert_eq!(
            read_connection(&path).unwrap_err(),
            AgentFailure::InvalidInput
        );
        std::fs::write(&path, "x".repeat(16385)).unwrap();
        assert!(read_connection(&path).is_err());
    }
}

pub(super) async fn run(mode: Mode) -> std::process::ExitCode {
    if std::env::var("FLOE_MANAGER_EVAL_APPROVED").as_deref() != Ok("1") {
        return unverified("explicit_live_opt_in_required");
    }
    let corpus = match load_corpus() {
        Ok(corpus) => corpus,
        Err(_) => return unverified("invalid_corpus"),
    };
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_owned());
    let Some(commit) = commit else {
        return unverified("commit_identity_unavailable");
    };
    let stage = std::env::var("FLOE_MANAGER_EVAL_STAGE").unwrap_or_else(|_| "baseline".into());
    if !matches!(stage.as_str(), "baseline" | "native" | "manager" | "card") {
        return unverified("invalid_stage");
    }
    let model_id = std::env::var("FLOE_MANAGER_EVAL_MODEL_ID").ok();
    if model_id
        .as_deref()
        .is_some_and(|value| !bounded(value, 128) || value.chars().any(char::is_control))
    {
        return unverified("invalid_model_identity");
    }
    let mut metadata = ReportMetadata {
        stage,
        commit,
        profile: "foundation-device",
        model_id,
        model_id_origin: "unavailable",
        configuration_hash: String::new(),
    };
    if metadata.model_id.is_some() {
        metadata.model_id_origin = "operator_configuration";
    }
    let consents = MemoryConsents::default();
    let mut all_accepted = true;
    match mode {
        Mode::Foundation => {
            let provider = FoundationModelProvider::synthetic();
            let profiles = provider.observe_profiles().await;
            let Some(profile) = profiles
                .iter()
                .find(|entry| entry.profile.id == metadata.profile && entry.profile.available)
            else {
                return unverified("foundation_unavailable");
            };
            metadata.configuration_hash =
                digest(serde_json::to_vec(&profile.profile).unwrap_or_default());
            let service = InferenceService::new(
                provider,
                super::learner::SmokeResolver,
                super::learner::SmokeAuthority,
            );
            let principal = PersonId::new().to_string();
            for case in &corpus.cases {
                for repetition in 1..=REPETITIONS {
                    let session_id = Uuid::new_v4();
                    let lineage = RecipientLineage::try_new(session_id, Uuid::new_v4()).unwrap();
                    match run_case(
                        &service, &corpus, case, repetition, &metadata, &principal, session_id,
                        lineage,
                    )
                    .await
                    {
                        Ok(accepted) => all_accepted &= accepted,
                        Err(_) => return unverified("fixture_projection_failure"),
                    }
                }
            }
        }
        Mode::Server => {
            let Some(path) = std::env::var_os("FLOE_MANAGER_EVAL_CONNECTION_FILE") else {
                return unverified("explicit_connection_file_required");
            };
            let saved = match read_connection(Path::new(&path)) {
                Ok(saved) => saved,
                Err(_) => return unverified("connection_file_rejected"),
            };
            let current = CurrentSavedConnectionStore::fixed(Some(saved.clone()));
            let provider = match RootModelProvider::from_current_connection(
                &current,
                &saved.person_id,
                &saved.device_id,
            ) {
                Ok(provider) => provider,
                Err(_) => return unverified("connection_admission_failed"),
            };
            metadata.profile = "server-model";
            let profiles = provider.observe_profiles().await;
            let Some(profile) = profiles
                .iter()
                .find(|entry| entry.profile.id == metadata.profile && entry.profile.available)
            else {
                return unverified("server_profile_unavailable");
            };
            let Some(recipient) = profile.profile.data_recipient.as_str() else {
                return unverified("external_recipient_required");
            };
            if std::env::var("FLOE_MANAGER_EVAL_RECIPIENT").as_deref() != Ok(recipient) {
                return unverified("exact_recipient_opt_in_required");
            }
            let person = match Uuid::parse_str(&saved.person_id)
                .ok()
                .and_then(PersonId::from_uuid)
            {
                Some(person) => person,
                None => return unverified("invalid_principal"),
            };
            metadata.configuration_hash = digest(
                serde_json::to_vec(&(&profile.profile, digest(&saved.base_url)))
                    .unwrap_or_default(),
            );
            let admission = SavedConnectionAdmission::new(
                current,
                saved.person_id.clone(),
                saved.device_id.clone(),
            );
            let authority =
                ContextualRecipientAuthority::new(&consents, admission, SystemConsentClock);
            let service = InferenceService::new(provider, super::learner::SmokeResolver, authority);
            for case in &corpus.cases {
                for repetition in 1..=REPETITIONS {
                    let session_id = Uuid::new_v4();
                    let lineage = RecipientLineage::try_new(session_id, Uuid::new_v4()).unwrap();
                    let consent = RecipientConsent::try_new(
                        person,
                        saved.device_id.clone(),
                        saved.client_id.clone(),
                        recipient,
                        metadata.profile,
                        CANONICAL_MODEL_PURPOSE,
                        CANONICAL_MODEL_CONSUMER,
                        vec![DataClass::Synthetic],
                        vec![],
                        lineage,
                        Uuid::new_v4(),
                        1,
                        Utc::now(),
                    );
                    let Ok(consent) = consent else {
                        return unverified("consent_construction_failed");
                    };
                    if grant_recipient_consent(&consents, consent).await.is_err() {
                        return unverified("consent_admission_failed");
                    }
                    match run_case(
                        &service,
                        &corpus,
                        case,
                        repetition,
                        &metadata,
                        &saved.person_id,
                        session_id,
                        lineage,
                    )
                    .await
                    {
                        Ok(accepted) => all_accepted &= accepted,
                        Err(_) => return unverified("fixture_projection_failure"),
                    }
                }
            }
        }
    }
    println!("{}", report_summary(&corpus, &metadata, all_accepted));
    if all_accepted {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
