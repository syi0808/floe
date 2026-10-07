//! An explicit integration exercise through the production AppHost owners.
//! The caller supplies an isolated, prepared profile; this never creates keys.
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use chrono::Utc;
use floe_app::{AppComposition, AppHost, ReadyOwners, RuntimeReadinessState};
use floe_conversation::{CommandReceipt, RunState, StartTurn};
use floe_execution::{BoxFuture, Cancellation, ExecutionScope};
use floe_kernel::{AgentFailure, CommandId, OwnerActor};
use serde_json::{Value, json};
use uuid::Uuid;

type Host = AppHost<AppComposition>;

pub(super) async fn run(
    with_expiry: bool,
    profile: Option<PathBuf>,
) -> Result<Value, AgentFailure> {
    let Some(profile) = profile else {
        return Ok(json!({"schema_version":1,"status":"SKIPPED",
            "reason":"explicit_isolated_prepared_profile_required",
            "required_argument":"--profile /absolute/path/to/people/PERSON/floe.db",
            "personal_data":false,"explicit_expiry":with_expiry}));
    };
    tokio::task::spawn_blocking(move || run_profile(&profile, with_expiry))
        .await
        .map_err(|_| AgentFailure::Interrupted)?
}

fn run_profile(profile: &Path, with_expiry: bool) -> Result<Value, AgentFailure> {
    let identity = floe_provider_adapters::local_identity_for_database(profile)
        .map_err(|_| AgentFailure::PolicyDenied)?
        .ok_or(AgentFailure::PolicyDenied)?;
    if !profile.is_absolute()
        || !std::fs::symlink_metadata(profile).is_ok_and(|metadata| metadata.file_type().is_file())
        || !PathBuf::from(format!("{}.agent-vaults", profile.display()))
            .join(identity.person_id.to_string())
            .join("vault.id")
            .is_file()
    {
        return Err(AgentFailure::InvalidInput);
    }
    let host = floe_app::open(profile.to_str().ok_or(AgentFailure::InvalidInput)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let outcome = (|| {
        prepare_runtime(&host)?;
        exercise(&host, with_expiry)
    })();
    host.shutdown().map_err(|_| AgentFailure::Interrupted)?;
    outcome
}

fn prepare_runtime(host: &Host) -> Result<(), AgentFailure> {
    let operation = Uuid::new_v4();
    let request = host
        .request(operation)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let mut result = request
        .services()
        .prepare_runtime(request.caller(), operation)
        .map_err(floe_app::RuntimePreparationCommandFailure::into_failure)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !result.done {
        if Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        std::thread::sleep(Duration::from_millis(50));
        result = request
            .services()
            .get_runtime_preparation(request.caller(), operation)?;
    }
    request
        .services()
        .acknowledge_runtime_preparation(request.caller(), operation)
        .map_err(floe_app::RuntimePreparationCommandFailure::into_failure)?;
    if let Some(failure) = result.failure {
        return Err(failure);
    }
    if request
        .services()
        .runtime_readiness(request.caller(), Uuid::new_v4())?
        .state
        != RuntimeReadinessState::Ready
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

fn owner_call<T: Send + 'static>(
    host: &Host,
    operation: impl for<'a> FnOnce(
        &'a ReadyOwners,
        &'a OwnerActor,
        &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<T, AgentFailure>>
    + Send
    + 'static,
) -> Result<T, AgentFailure> {
    let request_id = Uuid::new_v4();
    let request = host
        .request(request_id)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let owners = request.services().ready_owners(request.caller())?;
    let actor = request.caller().owner_actor();
    let scope = floe_app::host_scope(request_id, Cancellation::new(), Duration::from_secs(35));
    request
        .services()
        .execute_owner(async move { operation(&owners, &actor, &scope).await })
}

fn exercise(host: &Host, with_expiry: bool) -> Result<Value, AgentFailure> {
    let foreground = owner_call(host, |owners, actor, scope| {
        Box::pin(async move {
            let lease = owners.knowledge.foreground_lease()?;
            let overview = owners.connections.overview(actor, scope).await?;
            if !overview.sources.is_empty()
                || overview.gateways.iter().any(|gateway| {
                    !matches!(
                        gateway.state,
                        floe_connections::GatewayState::Unpaired
                            | floe_connections::GatewayState::Forgotten
                    ) || gateway.failure.is_some()
                        || gateway.remote_revocation_pending
                })
            {
                return Err(AgentFailure::PolicyDenied);
            }
            match owners.conversation.resume_session(actor, scope).await {
                Ok(None) => {}
                Ok(Some(_)) => return Err(AgentFailure::Conflict),
                Err(error) => return Err(error),
            }
            if !owners
                .knowledge
                .review(actor, scope)
                .await?
                .candidates
                .is_empty()
                || !owners
                    .knowledge
                    .read_context(actor, scope)
                    .await?
                    .memories
                    .is_empty()
            {
                return Err(AgentFailure::Conflict);
            }
            Ok(lease)
        })
    })?;
    let session = owner_call(host, |owners, actor, scope| {
        Box::pin(async move {
            use floe_conversation::{SessionStartAdmission as A, SessionStartFailure as F};
            // Uncertain admission fails this exercise without starting another Session.
            match owners
                .conversation
                .start_session(actor, CommandId::new(), scope)
                .await
            {
                Ok(A::Started(receipt) | A::Replayed(receipt)) => Ok(receipt),
                Ok(A::NotApplied(reason)) => Err(reason.reason()),
                Err(F::NotAdmitted(reason) | F::Indeterminate(reason)) => Err(reason),
            }
        })
    })?;
    let text = if with_expiry {
        "Please remember for later: fictional Alex prefers afternoon meetings. This preference expires at 2027-01-01T00:00:00Z, with no start date. Acknowledge this request without reading any external source."
    } else {
        "Please remember for later: fictional Alex prefers afternoon meetings. Acknowledge this request without reading any external source."
    };
    let command_id = CommandId::new();
    let command = StartTurn {
        command_id,
        session_id: session.session_id,
        expected_revision: session.session_revision,
        text: text.into(),
        continuation_ref: None,
        retry_of: None,
    };
    let admitted = match owner_call(host, move |owners, actor, scope| {
        owners.conversation.start_turn(actor, command, scope)
    }) {
        Ok(receipt) => receipt,
        Err(AgentFailure::StorageUnavailable) => owner_call(host, move |owners, actor, scope| {
            owners.conversation.read_command(actor, command_id, scope)
        })?
        .map(|receipt| CommandReceipt::from(&receipt))
        .ok_or(AgentFailure::StorageUnavailable)?,
        Err(error) => return Err(error),
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    let run = loop {
        let run_id = admitted.run_id;
        let receipt = owner_call(host, move |owners, actor, scope| {
            owners.conversation.read_run(actor, run_id, scope)
        })?
        .ok_or(AgentFailure::StorageUnavailable)?;
        if receipt.state.is_terminal() {
            break receipt;
        }
        if Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if run.state != RunState::Completed
        || run.issue.is_some()
        || run.output.is_none()
        || !run.unresolved_attempts.is_empty()
        || !run.task_refs.is_empty()
    {
        return Err(run.issue.unwrap_or(AgentFailure::Conflict));
    }
    let processed = owner_call(host, |owners, _, _| {
        owners.knowledge.run_next(Cancellation::new())
    })?;
    let review = owner_call(host, |owners, actor, scope| {
        owners.knowledge.review(actor, scope)
    })?;
    let [candidate] = review.candidates.as_slice() else {
        return Err(AgentFailure::Conflict);
    };
    if !processed || candidate.source_count == 0 {
        return Err(AgentFailure::Conflict);
    }
    let statement = candidate.statement.to_lowercase();
    let expected_expiry = if with_expiry {
        Some(
            chrono::DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
                .map_err(|_| AgentFailure::InvalidInput)?
                .with_timezone(&Utc),
        )
    } else {
        None
    };
    if candidate.valid_from.is_some()
        || candidate.valid_until != expected_expiry
        || !statement.contains("alex")
        || !statement.contains("afternoon")
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    drop(foreground);
    Ok(
        json!({"schema_version":1,"status":"passed","boundary":"device",
        "personal_data":false,"explicit_isolated_profile":true,"pending_candidates":1,
        "auto_approved":false,"explicit_expiry":with_expiry,
        "conversation_run_id":run.run_id,"candidate_id":candidate.candidate_id,
        "profile_retained":true}),
    )
}
