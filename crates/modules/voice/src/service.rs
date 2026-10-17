//! Generation-owned custody tasks survive a caller's observation timeout. The
//! service contains no media or provider driver; native privacy Stop must not
//! wait for these storage operations. Conversation retains all Work execution.
use crate::{
    VoiceCallRecord, VoiceIngressAdmission, VoiceIngressJournal, VoiceMutation,
    VoiceMutationAdmission, VoiceStartAdmission, VoiceStartIntent,
};
use floe_conversation::ConversationOwner;
use floe_execution::ExecutionScope;
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
    tasks: Mutex<JoinSet<()>>,
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
            tasks: Mutex::new(JoinSet::new()),
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
            .run(async { Ok(self.tasks.lock().await) })
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
            let result = journal.ingest(&owner, voice, event).await;
            // Even uncertain custody can have committed Work. Wake only after
            // the actual storage future finishes, not when its observer leaves.
            let wake_failure = if result.as_ref().map_or(true, |v| v.work.is_some()) {
                conversation.notify_surface_work(&owner).err()
            } else {
                None
            };
            result.map(|admission| VoiceIngressResult {
                admission,
                wake_failure,
            })
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
    pub fn close_admission(&self) {
        self.closing.store(true, Ordering::Release);
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
                if failed {
                    Err(AgentFailure::Interrupted)
                } else {
                    Ok(())
                }
            })
            .await
    }
}
