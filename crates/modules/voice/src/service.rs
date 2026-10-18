//! Generation-owned custody tasks survive a caller's observation timeout. The
//! media/provider I/O stays behind its ports; native privacy Stop must not
//! wait for these storage operations. Conversation retains all Work execution.
use crate::{
    VoiceCallRecord, VoiceIngressAdmission, VoiceIngressJournal, VoiceMutation,
    VoiceMutationAdmission, VoiceStartAdmission, VoiceStartIntent,
};
use floe_conversation::ConversationOwner;
use floe_execution::{Cancellation, ExecutionScope};
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor};
use floe_model_contract::LiveProviderEvent;
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinSet,
};
use uuid::Uuid;

/// Host-side in-flight custody bound, not a provider or conversation limit.
const MAX_CUSTODY_TASKS: usize = 64;

pub struct VoiceService {
    actor: OwnerActor,
    journal: Arc<dyn VoiceIngressJournal>,
    conversation: Arc<dyn ConversationOwner>,
    closing: AtomicBool,
    shutdown: Cancellation,
    tasks: Mutex<JoinSet<()>>,
    controllers: Arc<std::sync::Mutex<std::collections::HashSet<Uuid>>>,
    controller_failure: Arc<AtomicBool>,
}
#[derive(Clone, Debug)]
pub struct VoiceIngressResult {
    pub admission: VoiceIngressAdmission,
    /// A known receipt remains known even if retirement fences the wake hint.
    /// The canonical Work will be rediscovered by the next ready generation.
    pub wake_failure: Option<AgentFailure>,
}
impl VoiceService {
    pub fn new(
        actor: OwnerActor,
        journal: Arc<dyn VoiceIngressJournal>,
        conversation: Arc<dyn ConversationOwner>,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        Ok(Self {
            actor,
            journal,
            conversation,
            closing: AtomicBool::new(false),
            shutdown: Cancellation::new(),
            tasks: Mutex::new(JoinSet::new()),
            controllers: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            controller_failure: Arc::new(AtomicBool::new(false)),
        })
    }
    fn check(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        if self.closing.load(Ordering::Acquire) {
            return Err(AgentFailure::VaultUnavailable);
        }
        if actor != &self.actor {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    async fn custody<T: Send + 'static>(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
        operation: impl Future<Output = Result<T, CommandFailure<AgentFailure>>> + Send + 'static,
    ) -> Result<T, CommandFailure<AgentFailure>> {
        self.check(actor).map_err(CommandFailure::NotAdmitted)?;
        let mut tasks = scope
            .run(async {
                tokio::select! {
                    biased;
                    _ = self.shutdown.cancelled() => Err(AgentFailure::VaultUnavailable),
                    tasks = self.tasks.lock() => Ok(tasks),
                }
            })
            .await
            .map_err(CommandFailure::NotAdmitted)?;
        self.check(actor).map_err(CommandFailure::NotAdmitted)?;
        while let Some(result) = tasks.try_join_next() {
            result.map_err(|_| CommandFailure::NotAdmitted(AgentFailure::Interrupted))?;
        }
        if tasks.len() >= MAX_CUSTODY_TASKS {
            return Err(CommandFailure::NotAdmitted(AgentFailure::BudgetExceeded));
        }
        let (send, receive) = oneshot::channel();
        tasks.spawn(async move {
            let _ = send.send(operation.await);
        });
        drop(tasks);
        // Dropping the observer never drops the generation-owned custody task.
        scope
            .run(async { receive.await.map_err(|_| AgentFailure::Interrupted) })
            .await
            .map_err(CommandFailure::Indeterminate)?
    }
    pub async fn admit_start(
        &self,
        actor: &OwnerActor,
        intent: VoiceStartIntent,
        scope: &ExecutionScope,
    ) -> Result<VoiceStartAdmission, CommandFailure<AgentFailure>> {
        let journal = self.journal.clone();
        let owner = actor.clone();
        self.custody(actor, scope, async move {
            journal.admit_start(&owner, intent).await
        })
        .await
    }
    pub async fn mutate(
        &self,
        actor: &OwnerActor,
        request: VoiceMutation,
        scope: &ExecutionScope,
    ) -> Result<VoiceMutationAdmission, CommandFailure<AgentFailure>> {
        let journal = self.journal.clone();
        let owner = actor.clone();
        self.custody(actor, scope, async move {
            journal.mutate(&owner, request).await
        })
        .await
    }
    pub async fn ingest(
        &self,
        actor: &OwnerActor,
        voice: Uuid,
        event: LiveProviderEvent,
        scope: &ExecutionScope,
    ) -> Result<VoiceIngressResult, CommandFailure<AgentFailure>> {
        let journal = self.journal.clone();
        let owner = actor.clone();
        let conversation = self.conversation.clone();
        self.custody(actor, scope, async move {
            ingest_owned(
                journal.as_ref(),
                conversation.as_ref(),
                &owner,
                voice,
                event,
            )
            .await
        })
        .await
    }
    pub async fn read_call(
        &self,
        actor: &OwnerActor,
        voice: Uuid,
        scope: &ExecutionScope,
    ) -> Result<Option<VoiceCallRecord>, AgentFailure> {
        self.check(actor)?;
        scope.run(self.journal.read_call(actor, voice)).await
    }
    pub async fn read_event(
        &self,
        actor: &OwnerActor,
        voice: Uuid,
        kind: &str,
        id: &str,
        scope: &ExecutionScope,
    ) -> Result<Option<VoiceIngressAdmission>, AgentFailure> {
        self.check(actor)?;
        scope
            .run(self.journal.read_event(actor, voice, kind, id))
            .await
    }
    /// Records the Inference owner's exact terminal accounting independently of
    /// native Stop/remote-close receipts. Replay reads the original fact only.
    pub async fn settle_usage(
        &self,
        actor: &OwnerActor,
        voice: Uuid,
        receipt: floe_execution::budget::ModelAttemptReceipt,
        scope: &ExecutionScope,
    ) -> Result<floe_execution::budget::ModelAttemptReceipt, CommandFailure<AgentFailure>> {
        self.check(actor).map_err(CommandFailure::NotAdmitted)?;
        receipt.validate().map_err(CommandFailure::Indeterminate)?;
        let journal = self.journal.clone();
        let owner = actor.clone();
        // Admission owns the entire read/compare/mutate operation. An observer
        // leaving during the first read must not discard a known terminal bill,
        // and retirement must drain this task before sealing the journal.
        self.custody(actor, scope, async move {
            settle_usage(journal.as_ref(), &owner, voice, receipt).await
        })
        .await
    }

    /// Registers the call driver before it may start provider/native work. The
    /// observer may leave, but the driver remains in this generation's drain.
    /// Its borrowed capability allows exact-call accounting after admission is
    /// closed, without reopening public commands or exposing the journal.
    /// Return Ok only after required final custody. An error/panic remains a
    /// drain failure even if the observer has already left.
    pub async fn run_controller<D>(
        &self,
        binding: crate::VoiceBinding,
        scope: &ExecutionScope,
        driver: D,
    ) -> Result<(), CommandFailure<AgentFailure>>
    where
        D: for<'a> FnOnce(
                &'a VoiceController,
            ) -> floe_execution::BoxFuture<'a, Result<(), AgentFailure>>
            + Send
            + 'static,
    {
        binding.validate().map_err(CommandFailure::NotAdmitted)?;
        self.check(&binding.actor)
            .map_err(CommandFailure::NotAdmitted)?;
        let record = self
            .read_call(&binding.actor, binding.voice_session_id, scope)
            .await
            .map_err(CommandFailure::NotAdmitted)?
            .ok_or(CommandFailure::NotAdmitted(AgentFailure::NotFound))?;
        if record.intent.binding != binding
            || record.session.remote() != crate::RemoteState::NotStarted
            || record.session.stop_requested()
            || record.usage.is_some()
        {
            return Err(CommandFailure::NotAdmitted(AgentFailure::Conflict));
        }
        let lease = ControllerLease::claim(
            self.controllers.clone(),
            self.controller_failure.clone(),
            binding.voice_session_id,
        )
        .map_err(CommandFailure::NotAdmitted)?;
        let actor = binding.actor.clone();
        let mut controller = VoiceController {
            binding,
            shutdown: self.shutdown.child_scope(),
            journal: self.journal.clone(),
            conversation: self.conversation.clone(),
            _lease: lease,
        };
        self.custody(&actor, scope, async move {
            controller._lease.started = true;
            let result = driver(&controller).await;
            controller._lease.completed = result.is_ok();
            // Release registration and fence the child before reporting completion.
            drop(controller);
            result.map_err(CommandFailure::Admitted)
        })
        .await
    }

    pub fn close_admission(&self) {
        self.closing.store(true, Ordering::Release);
        self.shutdown
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    }
    pub async fn shutdown_and_drain(&self, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        self.close_admission();
        scope
            .run(async {
                let mut tasks = self.tasks.lock().await;
                let mut failed = false;
                while let Some(result) = tasks.join_next().await {
                    failed |= result.is_err();
                }
                if failed || self.controller_failure.load(Ordering::Acquire) {
                    Err(AgentFailure::Interrupted)
                } else {
                    Ok(())
                }
            })
            .await
    }
}

impl floe_inference::LiveDispatchCustody for VoiceService {
    fn lifetime_cancellation(&self, actor: &OwnerActor) -> Result<Cancellation, AgentFailure> {
        self.check(actor)?;
        Ok(self.shutdown.clone())
    }

    fn claim<'a>(
        &'a self,
        intent: &'a floe_inference::LiveDispatchIntent,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            intent.validate()?;
            self.check(&intent.actor)?;
            let record = self
                .read_call(&intent.actor, intent.voice_session_id, scope)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let saved = &record.intent;
            if saved.binding.actor != intent.actor
                || saved.binding.surface_session_id != intent.surface_session_id
                || saved.binding.media_lease_id != intent.media_lease_id
                || saved.binding.plan_id != intent.plan.operation_id
                || saved.model_binding != intent.plan.binding_digest
                || saved.selection_commitment != intent.plan.selection_commitment
                || saved.capabilities != intent.plan.capabilities
                || saved.limits != intent.plan.limits
                || saved.boundary != intent.plan.boundary
                || saved.context_digest != intent.projection_digest
                || saved.reservation != intent.reservation
            {
                return Err(AgentFailure::PolicyDenied);
            }
            if record.session.remote() != crate::RemoteState::NotStarted {
                return Ok(false);
            }
            let request = VoiceMutation {
                voice_session_id: intent.voice_session_id,
                command_id: Uuid::new_v5(&intent.voice_session_id, b"floe.voice.provider-dispatch"),
                expected_revision: record.revision,
                kind: crate::VoiceMutationKind::BeginRemoteDispatch {},
            };
            // Pure canonical policy preflight rejects unconfirmed native state
            // before submitting a command. Storage repeats it under the CAS.
            record.apply(&intent.actor, &request)?;
            // The exact confirmed native muted lease is mandatory. CAS races
            // and uncertain storage never become provider dispatch.
            let admission = self
                .mutate(&intent.actor, request, scope)
                .await
                .map_err(CommandFailure::into_failure)?;
            Ok(admission.fresh_remote_dispatch())
        })
    }
}

/// A generation-owned driver borrows this capability only for its owned task.
/// It cannot start another call or admit new input after retirement. Accounting
/// remains valid during drain; it is never a native Stop acknowledgement.
pub struct VoiceController {
    pub(crate) binding: crate::VoiceBinding,
    pub(crate) shutdown: Cancellation,
    pub(crate) journal: Arc<dyn VoiceIngressJournal>,
    pub(crate) conversation: Arc<dyn ConversationOwner>,
    _lease: ControllerLease,
}
impl VoiceController {
    pub fn binding(&self) -> &crate::VoiceBinding {
        &self.binding
    }
    pub fn cancellation(&self) -> &Cancellation {
        &self.shutdown
    }
    pub async fn settle_usage(
        &self,
        receipt: floe_execution::budget::ModelAttemptReceipt,
    ) -> Result<floe_execution::budget::ModelAttemptReceipt, CommandFailure<AgentFailure>> {
        settle_usage(
            self.journal.as_ref(),
            &self.binding.actor,
            self.binding.voice_session_id,
            receipt,
        )
        .await
    }
}
impl Drop for VoiceController {
    fn drop(&mut self) {
        self.shutdown
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    }
}
struct ControllerLease {
    failure: Arc<AtomicBool>,
    started: bool,
    completed: bool,
    registry: Arc<std::sync::Mutex<std::collections::HashSet<Uuid>>>,
    voice: Uuid,
}
impl ControllerLease {
    fn claim(
        registry: Arc<std::sync::Mutex<std::collections::HashSet<Uuid>>>,
        failure: Arc<AtomicBool>,
        voice: Uuid,
    ) -> Result<Self, AgentFailure> {
        if !registry.lock().unwrap().insert(voice) {
            return Err(AgentFailure::Conflict);
        }
        Ok(Self {
            registry,
            voice,
            failure,
            started: false,
            completed: false,
        })
    }
}
impl Drop for ControllerLease {
    fn drop(&mut self) {
        if self.started && !self.completed {
            self.failure.store(true, Ordering::Release);
        }
        self.registry.lock().unwrap().remove(&self.voice);
    }
}
async fn settle_usage(
    journal: &dyn VoiceIngressJournal,
    owner: &OwnerActor,
    voice: Uuid,
    receipt: floe_execution::budget::ModelAttemptReceipt,
) -> Result<floe_execution::budget::ModelAttemptReceipt, CommandFailure<AgentFailure>> {
    receipt.validate().map_err(CommandFailure::Indeterminate)?;
    let record =
        crate::custody::retry_local(|| journal.read_call(owner, voice), crate::custody::busy)
            .await
            .map_err(CommandFailure::Indeterminate)?
            .ok_or(CommandFailure::Indeterminate(AgentFailure::NotFound))?;
    if record.intent.binding.plan_id != receipt.attempt_id {
        return Err(CommandFailure::Indeterminate(AgentFailure::PolicyDenied));
    }
    if let Some(saved) = record.usage {
        return if saved == receipt {
            Ok(saved)
        } else {
            Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
        };
    }
    let request = VoiceMutation {
        voice_session_id: voice,
        command_id: Uuid::new_v5(&voice, b"floe.voice.usage-settlement"),
        expected_revision: record.revision,
        kind: crate::VoiceMutationKind::SettleUsage { receipt },
    };
    crate::custody::retry_local(
        || journal.mutate(owner, request.clone()),
        crate::custody::command_busy,
    )
    .await?
    .record
    .usage
    .ok_or(CommandFailure::Admitted(AgentFailure::StorageUnavailable))
}

#[cfg(test)]
mod controller_tests {
    use super::*;
    #[test]
    fn registration_drop_releases_identity_and_only_started_unfinished_drivers_fail_drain() {
        for (started, completed, failed) in [
            (false, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            let registry = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
            let failure = Arc::new(AtomicBool::new(false));
            let voice = Uuid::new_v4();
            let mut lease =
                ControllerLease::claim(registry.clone(), failure.clone(), voice).unwrap();
            assert!(matches!(
                ControllerLease::claim(registry.clone(), failure.clone(), voice),
                Err(AgentFailure::Conflict)
            ));
            lease.started = started;
            lease.completed = completed;
            drop(lease);
            assert!(registry.lock().unwrap().is_empty());
            assert_eq!(failure.load(Ordering::Acquire), failed);
        }
    }
}

pub(crate) async fn ingest_owned(
    journal: &dyn VoiceIngressJournal,
    conversation: &dyn ConversationOwner,
    owner: &OwnerActor,
    voice: Uuid,
    event: LiveProviderEvent,
) -> Result<VoiceIngressResult, CommandFailure<AgentFailure>> {
    let result = journal.ingest(owner, voice, event).await;
    // An uncertain local ACK can still have committed canonical Work.
    let wake_failure = if result.as_ref().map_or(true, |v| v.work.is_some()) {
        conversation.notify_surface_work(owner).err()
    } else {
        None
    };
    result.map(|admission| VoiceIngressResult {
        admission,
        wake_failure,
    })
}
