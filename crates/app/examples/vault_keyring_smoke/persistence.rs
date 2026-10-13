//! Shared owner-admission storage steps for the disposable Keychain diagnostic.
use floe_agent_contract::{AgentFailure, AgentMessage, DependencyCoverage, MessageRole};
use floe_conversation::{
    AgentSession, CanonicalTurnIntent, RunRecord, SessionStore, TurnAdmissionRequest, TurnInput,
    TurnMode,
};
use floe_kernel::{CommandId, PersonId, RunId};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider, VaultManagerConversationAdmission};

pub struct PersistenceProbe {
    pub session: AgentSession,
    pub run: RunRecord,
}

pub async fn persist_session<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person: PersonId,
) -> Result<PersistenceProbe, AgentFailure> {
    use floe_conversation::{SessionStartAdmission as A, SessionStartFailure as F};
    let receipt = match vault
        .start_conversation_session(floe_conversation::StartSessionRequest {
            principal: person.to_string(),
            command_id: CommandId::new(),
        })
        .await
    {
        Ok(A::Started(receipt) | A::Replayed(receipt)) => receipt,
        Ok(A::NotApplied(reason)) => return Err(reason.reason()),
        Err(F::NotAdmitted(reason) | F::Indeterminate(reason)) => return Err(reason),
    };
    let initial = vault.load(person, receipt.session_id).await?;
    vault.activate_conversation_executor().await?;
    let command_id = CommandId::new();
    let principal = person.to_string();
    let intent = CanonicalTurnIntent {
        session_id: initial.id,
        expected_revision: initial.revision,
        text: "Disposable encrypted persistence check".into(),
        mode: TurnMode::New,
        retry_of: None,
    };
    let request = TurnAdmissionRequest {
        expert_environment: floe_experts::RunExpertEnvironmentIdentity {
            revision: 1,
            // A fixed empty diagnostic environment; no model or Expert runs.
            digest: [41; 32],
        },
        run_id: RunId::new(),
        command_id,
        session_id: initial.id,
        expected_session_revision: initial.revision,
        principal: principal.clone(),
        device_id: "floe-vault-keyring-smoke".into(),
        request_digest: intent.digest(&principal)?,
        mode: TurnMode::New,
        retry_of: None,
        input: TurnInput::NewMessage(AgentMessage {
            message_id: command_id.as_uuid(),
            role: MessageRole::User,
            text: intent.text,
            call_id: None,
            coverage: DependencyCoverage::Independent,
        }),
    };
    let identity = floe_conversation::prompts::default_manager_identity(person)?;
    let run = match vault
        .admit_manager_conversation_turn(request.clone(), identity.clone())
        .await
        .map_err(command_failure)?
    {
        VaultManagerConversationAdmission::Created(run) => run,
        _ => return Err(AgentFailure::StorageUnavailable),
    };
    match vault
        .admit_manager_conversation_turn(request, identity)
        .await
        .map_err(command_failure)?
    {
        VaultManagerConversationAdmission::Existing(replayed) if replayed == run => {}
        _ => return Err(AgentFailure::StorageUnavailable),
    }
    let session = vault.load(person, initial.id).await?;
    if !session.messages.is_empty() || session.revision <= initial.revision {
        return Err(AgentFailure::StorageUnavailable);
    }
    let probe = PersistenceProbe { session, run };
    verify_session(vault, &probe).await?;
    vault.checkpoint().await?;
    Ok(probe)
}

fn command_failure(failure: floe_kernel::CommandFailure<AgentFailure>) -> AgentFailure {
    match failure {
        floe_kernel::CommandFailure::NotAdmitted(reason)
        | floe_kernel::CommandFailure::Admitted(reason)
        | floe_kernel::CommandFailure::NotApplied(reason)
        | floe_kernel::CommandFailure::Indeterminate(reason) => reason,
    }
}

pub async fn verify_session<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    expected: &PersistenceProbe,
) -> Result<(), AgentFailure> {
    if vault
        .load(expected.session.person_id, expected.session.id)
        .await?
        != expected.session
        || vault.conversation_run(expected.run.run_id).await?.as_ref() != Some(&expected.run)
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}
