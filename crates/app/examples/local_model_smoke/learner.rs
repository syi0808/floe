use std::{collections::HashMap, os::unix::fs::PermissionsExt, sync::Mutex};

use chrono::Utc;
use floe_agent_contract::AgentFailure;
use floe_conversation::{AgentMessage, AgentOutcome, SessionStore};
use floe_execution::Cancellation;
use floe_kernel::PersonId;
use floe_knowledge::{
    InferenceLearnerModel, LearnerJobState, LearnerModelAvailability, LearnerService,
    TransportLearnerModel,
};
use floe_provider_adapters::models::learner::FoundationLearnerTransport;
use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
struct SmokeKeys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

/// The bundled on-device model bound to the Learner role, as the host binds it.
struct SmokeTransport(FoundationLearnerTransport);

impl Default for SmokeTransport {
    fn default() -> Self {
        Self(FoundationLearnerTransport::new("floe.learner.synthetic"))
    }
}

impl floe_inference::ModelTransport for SmokeTransport {
    fn placement(&self) -> floe_agent_contract::ModelPlacement {
        self.0.placement()
    }

    async fn generate(
        &self,
        request: floe_inference::ModelTransportRequest,
    ) -> Result<floe_inference::ModelTransportResponse, AgentFailure> {
        self.0.generate(request).await
    }
}

impl LearnerModelAvailability for SmokeTransport {
    fn profile_id(&self) -> &str {
        self.0.profile_id()
    }

    fn is_available(&self) -> Result<bool, AgentFailure> {
        Ok(true)
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

pub(super) async fn run(with_expiry: bool) -> Result<Value, AgentFailure> {
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
            text: if with_expiry {
                "Please remember for later: fictional Alex prefers afternoon meetings. This preference expires at 2027-01-01T00:00:00Z, with no start date."
            } else {
                "Please remember for later: fictional Alex prefers afternoon meetings."
            }.into(),
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
    let transport = SmokeTransport::default();
    let profile_id = LearnerModelAvailability::profile_id(&transport).to_owned();
    let model = InferenceLearnerModel::new(TransportLearnerModel::new(transport));
    let processed = LearnerService {
        model: &model,
        repository: &vault,
    }
    .run_next(Cancellation::default())
    .await?;
    let settled = vault
        .enqueue_learner_review(queued[0].input.clone(), Utc::now())
        .await?;
    if !processed || settled.state != LearnerJobState::Completed {
        return Err(settled.last_failure.unwrap_or(AgentFailure::Conflict));
    }
    let pending = vault.memory_review_snapshot().await?;
    if pending.candidates.len() != usize::from(settled.candidate_id.is_some())
        || pending.candidates.first().map(|candidate| candidate.id) != settled.candidate_id
        || (with_expiry && pending.candidates.is_empty())
    {
        return Err(AgentFailure::Conflict);
    }
    if let Some(candidate) = pending.candidates.first() {
        let floe_knowledge::KnowledgePayload::Memory { value } = &candidate.payload else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        let statement = value.statement.to_lowercase();
        let expected_expiry = if with_expiry {
            Some(
                chrono::DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
                    .map_err(|_| AgentFailure::InvalidInput)?
                    .with_timezone(&Utc),
            )
        } else {
            None
        };
        if value.valid_from.is_some()
            || value.valid_until != expected_expiry
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
        "explicit_expiry": with_expiry,
    }))
}
