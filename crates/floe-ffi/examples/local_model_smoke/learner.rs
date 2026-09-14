use std::{collections::HashMap, os::unix::fs::PermissionsExt, sync::Mutex};

use chrono::Utc;
use floe_agent::{AgentFailure, AgentMessage, AgentOutcome, Cancellation, SessionStore};
use floe_core::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
use floe_domain::PersonId;
use floe_inference::{ModelProfile, PlannedRoute};
use floe_infra::learner_model::FoundationLearnerTransport;
use floe_knowledge::{
    InferenceLearnerModel, LearnerBudget, LearnerInferenceResponse, LearnerInferenceTransport,
    LearnerJobSettlement, LearnerModelRequest, LearnerRuntime,
};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
struct SmokeKeys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

struct SmokeTransport;

impl LearnerInferenceTransport for SmokeTransport {
    fn profile(&self) -> Result<ModelProfile, AgentFailure> {
        FoundationLearnerTransport.profile()
    }

    async fn generate(
        &self,
        route: PlannedRoute,
        request: LearnerModelRequest,
    ) -> Result<LearnerInferenceResponse, AgentFailure> {
        let response = FoundationLearnerTransport.generate(route, request).await?;
        println!(
            "{}",
            json!({"personal_data":false,"synthetic_learner_answer":response.text})
        );
        Ok(response)
    }
}

impl VaultKeyProvider for SmokeKeys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .get(&(person, vault))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        let mut keys = self.0.lock().map_err(|_| AgentFailure::VaultUnavailable)?;
        if keys.contains_key(&(person, vault)) {
            return Err(AgentFailure::VaultUnavailable);
        }
        keys.insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

pub(super) async fn run() -> Result<Value, AgentFailure> {
    let root = tempfile::tempdir().map_err(|_| AgentFailure::StorageUnavailable)?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, SmokeKeys::default()).await?;
    let mut session = vault.create_session().await?;
    let turn_id = Uuid::new_v4();
    session.messages = vec![
        AgentMessage::User {
            turn_id,
            text: "Please remember for later: fictional Alex prefers afternoon meetings.".into(),
        },
        AgentMessage::Assistant {
            turn_id,
            text: "I will prepare the fictional preference for review.".into(),
        },
    ];
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault
        .governed_general_store(session.id)
        .compare_and_swap(&session, 0)
        .await?;
    let now = Utc::now();
    let queued = vault.discover_explicit_learner_reviews(now, 1).await?;
    if queued.len() != 1 {
        return Err(AgentFailure::Conflict);
    }
    let claimed = vault
        .claim_learner_review(Utc::now())
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if claimed.id != queued[0].id {
        return Err(AgentFailure::Conflict);
    }
    let transport = SmokeTransport;
    let profile_id = transport.profile()?.id;
    let model = InferenceLearnerModel::new(transport);
    let candidate = LearnerRuntime {
        model: &model,
        candidates: &vault,
        budget: LearnerBudget::default(),
        extractor_version: floe_knowledge::prompts::LEARNER_EXTRACTOR_VERSION,
        prompt_version: floe_knowledge::prompts::LEARNER_PROMPT_VERSION,
    }
    .review(claimed.input, Cancellation::default())
    .await?;
    vault
        .settle_learner_review(
            claimed.id,
            claimed.attempts,
            LearnerJobSettlement::Completed {
                candidate_id: candidate.as_ref().map(|candidate| candidate.id),
            },
            Utc::now(),
        )
        .await?;
    let pending = vault.memory_review_snapshot().await?;
    if pending.candidates.len() != usize::from(candidate.is_some()) {
        return Err(AgentFailure::Conflict);
    }
    if let Some(candidate) = &candidate {
        let floe_knowledge::KnowledgePayload::Memory { value } = &candidate.payload else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        let statement = value.statement.to_lowercase();
        if value.valid_from.is_some()
            || value.valid_until.is_some()
            || !statement.contains("alex")
            || !statement.contains("afternoon")
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(json!({
        "schema_version": 1,
        "status": "passed",
        "profile": profile_id,
        "personal_data": false,
        "disposable_encrypted_vault": true,
        "pending_candidates": pending.candidates.len(),
        "auto_approved": false,
    }))
}
