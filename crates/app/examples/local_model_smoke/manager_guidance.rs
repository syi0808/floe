use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use floe_agent_contract::{
    A2A_PROTOCOL_VERSION, AGENT_SCHEMA_VERSION, AgentCard, AgentContext, AgentDefinition,
    AgentFailure, AllowedCatalog, AuthorizedModelProjection, BoxFuture, DataClass,
    DependencyCoverage, ModelCapabilities, ModelConversation, ModelConversationEntry,
    ModelPlanRequest, ModelPort, ModelProjectionOutcome, ModelProjectionPort,
    ModelProjectionRequest, ModelRequest, ModelStep, PreparedModelPlan, ProcessingBoundary,
};
use floe_conversation::{ConversationModelProjection, prompts::manager_role_spec};
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{PersonId, TraceContext};
use floe_provider_adapters::gateway::GatewayCredentialStore;
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

#[derive(Clone, Copy)]
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
    fn read_turn_coverage<'a>(
        &'a self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> floe_execution::BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
        let known = session_id == self.session_id && self.history_ids.contains(&turn_id);
        Box::pin(async move {
            Ok(if known {
                DependencyCoverage::Independent
            } else {
                DependencyCoverage::Unknown
            })
        })
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
    plan: &PreparedModelPlan,
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
        super::support::SmokeResolver,
        session_id,
        context(),
        vec![DataClass::Synthetic],
        floe_experts::RunExpertEnvironmentIdentity {
            revision: catalog.revision,
            digest: Sha256::digest(
                serde_json::to_vec(catalog).map_err(|_| AgentFailure::InvalidInput)?,
            )
            .into(),
        },
    )?;
    let projection = projector
        .project(
            ModelProjectionRequest {
                principal: principal.into(),
                plan: plan.clone(),
                projection_operation_id: Uuid::new_v4(),
                role: manager_role_spec(),
                conversation,
                catalog: catalog.clone(),
                max_output_bytes: OUTPUT_BYTES,
                correction: None,
            },
            scope,
        )
        .await?;
    let projection = match projection {
        ModelProjectionOutcome::Ready(projection) => projection,
        ModelProjectionOutcome::NeedsSourceReview(_) => {
            return Err(AgentFailure::AccessReviewRequired);
        }
    };
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

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

struct ReportMetadata {
    stage: String,
    commit: String,
    boundary: &'static str,
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
    failure: Option<AgentFailure>,
) -> Value {
    let kind = case.accepted_kind;
    let agents = &case.accepted_agents;
    let accepted = failure.is_none() && classify_batch(steps, kind, agents);
    json!({
        "schema_version": 1, "case_id": case.id, "repetition": repetition, "stage": metadata.stage, "commit_sha": metadata.commit,
        "corpus_sha256": digest(CORPUS), "provider": if metadata.boundary == "device" { "device" } else { "server" },
        "boundary": metadata.boundary, "model_id": metadata.model_id, "model_id_origin": metadata.model_id_origin,
        "configuration_sha256": metadata.configuration_hash,
        "prompt_components": projection.envelope.manifest.prompt_components,
        "stable_instructions_sha256": projection.envelope.manifest.stable_prompt_sha256,
        "run_frame_sha256": projection.envelope.manifest.run_frame_sha256,
        "expert_environment": projection.envelope.manifest.expert_environment,
        "ordered_cards": projection.envelope.manifest.agent_cards,
        "phase": "selection", "batch_steps": steps, "choice_accepted": accepted,
        "status": if failure.is_some() { "EXECUTION_FAILURE" } else if accepted { "REVIEW_REQUIRED" } else { "BEHAVIOR_FAILURE" },
        "failure": failure, "behavior_review": "pending", "rubric": case.rubric, "personal_data": false,
        "accepted_kind": kind, "accepted_agents": agents, "review_focus": case.review_focus,
        "synthetic_result_replay": false, "real_expert_execution": false,
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
        "provider": if metadata.boundary == "device" { "device" } else { "server" },
        "boundary": metadata.boundary, "model_id": metadata.model_id, "model_id_origin": metadata.model_id_origin,
        "configuration_sha256": metadata.configuration_hash,
        "cases": corpus.cases.len(), "synthesis_cases": 0, "deferred_synthesis_cases": synthesis_cases,
        "coverage": "selection_only", "repetitions": REPETITIONS,
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
    device_id: &str,
    journal: &super::support::DiagnosticJournal,
) -> Result<bool, AgentFailure> {
    let catalog = build_catalog(corpus, case)?;
    let conversation = conversation(case);
    let accepted = {
        let ledger = BudgetLedger::new(BudgetConfig::new(8192, 1_000_000), ModelUsage::default());
        let scope = ExecutionScope::root(
            Cancellation::default(),
            tokio::time::Instant::now() + Duration::from_secs(40),
            ledger.work_lease(),
            TraceContext::new(session_id),
        );
        let prepared = model
            .prepare(
                ModelPlanRequest {
                    principal: principal.into(),
                    device_id: device_id.into(),
                    purpose: "everyday_assistance".into(),
                    consumer: "conversation.root".into(),
                    required_capabilities: ModelCapabilities::chat(),
                },
                &scope,
            )
            .await?;
        let expected_boundary = if metadata.boundary == "device" {
            ProcessingBoundary::Device
        } else {
            ProcessingBoundary::Gateway
        };
        if prepared.plan().boundary != expected_boundary {
            return Err(AgentFailure::PolicyDenied);
        }
        let projection = project_case(
            &catalog,
            conversation.clone(),
            session_id,
            principal,
            prepared.plan(),
            &scope,
        )
        .await?;
        let request = ModelRequest {
            reservation_ceiling: floe_execution::budget::ModelReservationCeiling::for_lease(
                scope.budget(),
            ),
            attempt_id: Uuid::new_v4(),
            principal: principal.into(),
            device_id: device_id.into(),
            projection: projection.clone(),
            catalog: catalog.clone(),
            purpose: "everyday_assistance".into(),
            consumer: "conversation.root".into(),
            replay: vec![],
        };
        let (steps, failure) =
            match super::support::invoke(prepared.as_ref(), request, &scope, journal).await {
                Ok(response) => (response.steps, None),
                Err(failure) => (vec![], Some(failure)),
            };
        let report = report_case(
            case,
            repetition,
            metadata,
            &projection,
            &catalog,
            &steps,
            failure,
        );
        let shape_ok = report["choice_accepted"] == true;
        println!("{report}");
        shape_ok
    };
    if case.result_fixture.is_some() {
        println!(
            "{}",
            json!({
                "schema_version":1, "case_id":case.id, "repetition":repetition,
                "phase":"synthesis", "status":"UNVERIFIED", "reason":"s3_real_task_receipt_reconstruction",
                "synthetic_result_replay":false, "real_expert_execution":false, "personal_data":false,
            })
        );
    }
    Ok(accepted)
}

fn unverified(reason: &str) -> std::process::ExitCode {
    println!(
        "{}",
        json!({"schema_version":1,"status":"UNVERIFIED","reason":reason,"personal_data":false})
    );
    std::process::ExitCode::FAILURE
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
    let (principal, device_id, service, _synthetic_profile) = match mode {
        Mode::Foundation => {
            let profile = match super::support::SyntheticProfile::create().await {
                Ok(profile) => profile,
                Err(_) => return unverified("isolated_profile_unavailable"),
            };
            (
                profile.actor.person_id.to_string(),
                profile.actor.device_id.clone(),
                profile.model(),
                Some(profile),
            )
        }
        Mode::Server => {
            let Some(database) = std::env::var_os("FLOE_MANAGER_EVAL_DATABASE") else {
                return unverified("explicit_existing_database_required");
            };
            let database = Path::new(&database);
            if !database.is_absolute() || !database.is_file() {
                return unverified("existing_database_required");
            }
            let identity = match floe_provider_adapters::local_identity_for_database(database) {
                Ok(Some(identity)) => identity,
                _ => return unverified("verified_profile_identity_required"),
            };
            let Some(person) = PersonId::from_uuid(identity.person_id) else {
                return unverified("invalid_principal");
            };
            let root = PathBuf::from(format!("{}.agent-vaults", database.display()));
            let vault = match floe_vault::EncryptedAgentVault::open(
                &root,
                person,
                floe_vault::KeyringVaultKeys,
            )
            .await
            {
                Ok(vault) => Arc::new(vault),
                _ => return unverified("existing_vault_unavailable"),
            };
            let store = GatewayCredentialStore::new(vault);
            (
                person.to_string(),
                identity.device_id,
                super::support::model(store),
                None,
            )
        }
    };
    let preflight_scope = ExecutionScope::root(
        Cancellation::default(),
        tokio::time::Instant::now() + Duration::from_secs(30),
        BudgetLedger::new(BudgetConfig::new(8192, 1_000_000), ModelUsage::default()).work_lease(),
        TraceContext::new(Uuid::new_v4()),
    );
    let prepared = match service
        .prepare(
            ModelPlanRequest {
                principal: principal.clone(),
                device_id: device_id.clone(),
                purpose: "everyday_assistance".into(),
                consumer: "conversation.root".into(),
                required_capabilities: ModelCapabilities::chat(),
            },
            &preflight_scope,
        )
        .await
    {
        Ok(prepared) => prepared,
        Err(_) => return unverified("model_planning_failed"),
    };
    let expected = match mode {
        Mode::Foundation => ProcessingBoundary::Device,
        Mode::Server => ProcessingBoundary::Gateway,
    };
    if prepared.plan().boundary != expected {
        return unverified("requested_diagnostic_boundary_not_selected");
    }
    let metadata = ReportMetadata {
        stage,
        commit,
        boundary: if expected == ProcessingBoundary::Device {
            "device"
        } else {
            "gateway"
        },
        model_id_origin: if model_id.is_some() {
            "operator_configuration"
        } else {
            "unavailable"
        },
        model_id,
        configuration_hash: digest(
            serde_json::to_vec(&(
                prepared.plan().boundary,
                prepared.plan().binding_digest,
                &prepared.plan().capabilities,
            ))
            .unwrap_or_default(),
        ),
    };
    let journal = match super::support::DiagnosticJournal::new() {
        Ok(journal) => journal,
        Err(_) => return unverified("durable_journal_unavailable"),
    };
    println!(
        "{}",
        json!({"journal":journal.path(),"personal_data":false})
    );
    let mut all_accepted = true;
    for case in &corpus.cases {
        for repetition in 1..=REPETITIONS {
            match run_case(
                &service,
                &corpus,
                case,
                repetition,
                &metadata,
                &principal,
                Uuid::new_v4(),
                &device_id,
                &journal,
            )
            .await
            {
                Ok(accepted) => all_accepted &= accepted,
                Err(_) => return unverified("fixture_projection_failure"),
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
