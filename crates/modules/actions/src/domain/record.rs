use chrono::{DateTime, Duration, Utc};
use floe_agent_contract::{PackageKind, PackageRef, TaskExecutionReceiptRef};
use floe_context_contract::{CalendarProvider, ConnectionId, ContextDependency, SourceAuthority};
use floe_day::{CalendarExternalRevision, Event, SourceRef, TimedSchedule};
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub type ActionDigest = [u8; 32];
pub const MAX_ACTION_BYTES: usize = 65_536;

pub fn action_digest<T: Serialize>(domain: &[u8], value: &T) -> Result<ActionDigest, AgentFailure> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?;
    if bytes.len() > MAX_ACTION_BYTES { return Err(AgentFailure::BudgetExceeded); }
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(bytes);
    Ok(digest.finalize().into())
}

pub(crate) fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.trim() == value && !value.chars().any(char::is_control)
}

pub fn action_uuid(domain: &[u8], person: PersonId, command: Uuid) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(person.0.as_bytes());
    digest.update(command.as_bytes());
    let hash = digest.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 15) | 0x50;
    bytes[8] = (bytes[8] & 63) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarDestination {
    pub provider: CalendarProvider,
    pub connection_id: ConnectionId,
    pub connection_revision: u64,
    pub calendar_id: String,
    pub calendar_name: String,
}

impl CalendarDestination {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.provider != CalendarProvider::EventKit || self.connection_revision == 0
            || !bounded(self.connection_id.as_str(), 256) || !bounded(&self.calendar_id, 512)
            || !bounded(&self.calendar_name, 512) { return Err(AgentFailure::InvalidInput); }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarTarget {
    pub original: Event,
}

impl CalendarTarget {
    pub fn source(&self) -> Result<&floe_day::CalendarSource, AgentFailure> {
        match &self.original.source {
            SourceRef::Calendar(value) if value.can_modify && bounded(&value.external_id, 1024)
                && value.provider == CalendarProvider::EventKit && value.external_revision.is_valid() && self.original.deleted_at.is_none() => Ok(value),
            _ => Err(AgentFailure::InvalidInput),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarEffect {
    Create { destination: CalendarDestination, title: String, schedule: TimedSchedule },
    Update { destination: CalendarDestination, target: CalendarTarget, title: String, schedule: TimedSchedule },
    Delete { destination: CalendarDestination, target: CalendarTarget },
}

impl CalendarEffect {
    pub fn destination(&self) -> &CalendarDestination {
        match self { Self::Create{destination,..}|Self::Update{destination,..}|Self::Delete{destination,..} => destination }
    }
    pub fn target(&self) -> Option<&CalendarTarget> {
        match self { Self::Create{..}=>None, Self::Update{target,..}|Self::Delete{target,..}=>Some(target) }
    }
    pub fn write(&self) -> Option<(&str, &TimedSchedule)> {
        match self { Self::Create{title,schedule,..}|Self::Update{title,schedule,..}=>Some((title,schedule)), Self::Delete{..}=>None }
    }
    pub fn validate(&self, person: PersonId) -> Result<(), AgentFailure> {
        self.destination().validate()?;
        if let Some((title, schedule)) = self.write() {
            if !bounded(title, 1024) || !bounded(&schedule.timezone, 128)
                || schedule.ends_at <= schedule.starts_at || schedule.ends_at - schedule.starts_at > Duration::hours(24) {
                return Err(AgentFailure::InvalidInput);
            }
        }
        if let Some(target) = self.target() {
            let source = target.source()?;
            if target.original.person_id != person || source.provider != self.destination().provider
                || source.connection_id != self.destination().connection_id
                || source.calendar_id != self.destination().calendar_id { return Err(AgentFailure::PolicyDenied); }
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<ActionDigest, AgentFailure> { action_digest(b"floe.actions.effect.v1\0", self) }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionOrigin {
    Direct { command_id: Uuid, actor_device_id: String },
    Expert { task_id: Uuid, invocation_id: Uuid, package: PackageRef, installation_id: Uuid,
        assignment_id: Uuid, definition_revision: u64, evidence_ref: TaskExecutionReceiptRef, artifact_id: Uuid },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSourceFence {
    pub connection_id: ConnectionId,
    pub revision: u64,
    pub authority: SourceAuthority,
    pub execution_owner: String,
    pub resources: Vec<String>,
    pub native_subject_fingerprint: String,
}

impl ActionSourceFence {
    pub fn validate(&self, effect: &CalendarEffect, device: &str) -> Result<(), AgentFailure> {
        let destination = effect.destination();
        if self.connection_id != destination.connection_id || self.revision != destination.connection_revision
            || self.revision == 0 || !self.authority.is_valid()
            || !matches!(self.execution_owner.strip_prefix("apple:").or_else(||self.execution_owner.strip_prefix("macos:")), Some(owner) if owner == device)
            || !bounded(&self.native_subject_fingerprint, 512) || self.resources.is_empty()
            || !self.resources.iter().any(|value|value == &destination.calendar_id)
            || self.resources.iter().any(|value|!bounded(value,512))
            || self.resources.windows(2).any(|pair|pair[0]>=pair[1]) { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }
    pub fn digest(&self) -> Result<ActionDigest, AgentFailure> { action_digest(b"floe.actions.source.v1\0",self) }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReviewRef {
    pub id: Uuid,
    pub action_id: Uuid,
    pub effect_digest: ActionDigest,
    pub source_digest: ActionDigest,
    pub authority_revision: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionAuthorization {
    DirectInstruction { command_id: Uuid, person_id: PersonId, device_id: String, effect_digest: ActionDigest, authority_revision: u64, expires_at: DateTime<Utc> },
    ReviewedDecision { command_id: Uuid, person_id: PersonId, device_id: String, review: ActionReviewRef, decided_at: DateTime<Utc> },
    StandingPolicy { person_id: PersonId, effect_digest: ActionDigest, authority_revision: u64, expires_at: DateTime<Utc> },
}

impl ActionAuthorization {
    pub fn authority_revision(&self) -> u64 {
        match self { Self::DirectInstruction{authority_revision,..}|Self::StandingPolicy{authority_revision,..}=>*authority_revision,
            Self::ReviewedDecision{review,..}=>review.authority_revision }
    }
    pub fn validate_for(&self, record: &ActionRecord, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        let valid = match self {
            Self::DirectInstruction{command_id,person_id,device_id,effect_digest,authority_revision,expires_at} =>
                matches!(&record.origin,ActionOrigin::Direct{command_id:original,actor_device_id} if original==command_id && actor_device_id==device_id)
                && *person_id==record.person_id && device_id==&record.device_id && *effect_digest==record.effect_digest && *authority_revision>0 && *expires_at==record.expires_at,
            Self::ReviewedDecision{command_id,person_id,device_id,review,decided_at}=>!command_id.is_nil()
                && *person_id==record.person_id && device_id==&record.device_id && review==&record.review
                && *decided_at>=record.created_at && *decided_at<record.expires_at,
            Self::StandingPolicy{person_id,effect_digest,authority_revision,expires_at}=>matches!(record.origin,ActionOrigin::Expert{..})
                && matches!(record.effect,CalendarEffect::Create{..}) && *person_id==record.person_id && *effect_digest==record.effect_digest
                && *authority_revision>0 && *expires_at==record.expires_at,
        };
        if !valid || now<record.created_at || now>=record.expires_at { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum ActionBlockedReason { PermissionDenied, PolicyDenied, SourceChanged, ExecutorUnavailable, ScheduleConflict }

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum ActionUnknownReason { Timeout, ResponseLost, InvalidReceipt, CancelledAfterDispatch, InconclusiveLookup, NativeOperationPending, NativeReceiptUnavailable }

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum ActionNotAppliedReason { PermissionDenied, ProviderRejected, ProviderUnavailable }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EffectIdentity {
    pub execution_id: Uuid,
    pub effect_digest: ActionDigest,
    pub person_id: PersonId,
    pub device_id: String,
    pub executor_generation: u64,
    pub connection_id: ConnectionId,
    pub calendar_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarWriteResult {
    pub external_id: String,
    pub external_revision: CalendarExternalRevision,
    pub title: String,
    pub schedule: TimedSchedule,
    pub can_modify: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum CommittedCalendarEffect {
    Created { event: CalendarWriteResult },
    Updated { target: CalendarTarget, event: CalendarWriteResult },
    Deleted { target: CalendarTarget },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum CalendarReceiptEvidence {
    NativeAcknowledgement { host_epoch: Uuid, receipt_id: Uuid },
    UniqueCreateMarker { observed_at: DateTime<Utc>, marker: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarEffectReceipt {
    pub identity: EffectIdentity,
    pub effect: CommittedCalendarEffect,
    pub evidence: CalendarReceiptEvidence,
    pub committed_at: DateTime<Utc>,
}

impl CalendarEffectReceipt {
    pub fn digest(&self)->Result<ActionDigest,AgentFailure>{action_digest(b"floe.actions.receipt.v1\0",self)}
    pub fn validate_for(&self, intent:&ExecutionIntent)->Result<(),AgentFailure>{
        if self.identity!=intent.identity() || self.committed_at<intent.prepared_at {return Err(AgentFailure::PolicyDenied);}
        match &self.evidence {
            CalendarReceiptEvidence::NativeAcknowledgement{host_epoch,receipt_id} if !host_epoch.is_nil() && !receipt_id.is_nil()=>{},
            CalendarReceiptEvidence::UniqueCreateMarker{observed_at,marker} if matches!(intent.effect,CalendarEffect::Create{..})
                && *observed_at>=intent.prepared_at && marker==&format!("floe://calendar-action/{}/{}",intent.person_id,intent.execution_id)=>{},
            _=>return Err(AgentFailure::PolicyDenied),
        }
        let result = match (&intent.effect,&self.effect) {
            (CalendarEffect::Create{..},CommittedCalendarEffect::Created{event})=>Some(event),
            (CalendarEffect::Update{target,..},CommittedCalendarEffect::Updated{target:actual,event}) if target==actual
                && target.source()?.external_id==event.external_id=>Some(event),
            (CalendarEffect::Delete{target,..},CommittedCalendarEffect::Deleted{target:actual}) if target==actual=>None,
            _=>return Err(AgentFailure::PolicyDenied),
        };
        if let Some(result)=result {
            let Some((title,schedule))=intent.effect.write() else{return Err(AgentFailure::PolicyDenied)};
            if !bounded(&result.external_id,1024) || !result.external_revision.is_valid()
                || result.title!=title || &result.schedule!=schedule {return Err(AgentFailure::PolicyDenied);}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NotAppliedProof {
    pub identity: EffectIdentity,
    pub host_epoch: Uuid,
    pub invocation_id: Uuid,
    pub reason: ActionNotAppliedReason,
    pub rejected_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag="status",rename_all="snake_case",deny_unknown_fields)]
pub enum CalendarEffectOutcome {
    Committed { receipt: CalendarEffectReceipt },
    NotApplied { proof: NotAppliedProof },
    Unknown { identity: EffectIdentity, reason: ActionUnknownReason },
}

impl CalendarEffectOutcome {
    pub fn validate_for(&self,intent:&ExecutionIntent)->Result<(),AgentFailure>{
        match self {
            Self::Committed{receipt}=>receipt.validate_for(intent),
            Self::NotApplied{proof} if proof.identity==intent.identity() && !proof.host_epoch.is_nil()
                && !proof.invocation_id.is_nil() && proof.rejected_at>=intent.prepared_at=>Ok(()),
            Self::Unknown{identity,..} if identity==&intent.identity()=>Ok(()),
            _=>Err(AgentFailure::PolicyDenied),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionIntent {
    pub action_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub execution_id: Uuid,
    pub effect_digest: ActionDigest,
    pub effect: CalendarEffect,
    pub source: ActionSourceFence,
    pub authorization: ActionAuthorization,
    pub executor_generation: u64,
    pub prepared_at: DateTime<Utc>,
}

impl ExecutionIntent {
    pub fn identity(&self)->EffectIdentity { EffectIdentity{execution_id:self.execution_id,effect_digest:self.effect_digest,
        person_id:self.person_id,device_id:self.device_id.clone(),executor_generation:self.executor_generation,
        connection_id:self.effect.destination().connection_id.clone(),calendar_id:self.effect.destination().calendar_id.clone()} }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag="state",rename_all="snake_case",deny_unknown_fields)]
pub enum ActionCollectionState { Pending{ticket_id:Uuid}, Collected{day_projection_ref:String} }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag="state",rename_all="snake_case",deny_unknown_fields)]
pub enum ActionState {
    PendingReview, Approved, Rejected, Cancelled, Expired,
    Executing{execution_id:Uuid}, Blocked{reason:ActionBlockedReason},
    Failed{reason:ActionNotAppliedReason,not_applied_proof:NotAppliedProof},
    Unknown{reason:ActionUnknownReason},
    Succeeded{receipt:CalendarEffectReceipt,collection:ActionCollectionState},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionTicket {
    pub id:Uuid, pub person_id:PersonId, pub action_id:Uuid, pub execution_id:Uuid,
    pub receipt_digest:ActionDigest, pub revision:u64, pub state:ActionCollectionState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRecord {
    pub id:Uuid, pub person_id:PersonId, pub device_id:String, pub revision:u64,
    pub origin:ActionOrigin, pub effect:CalendarEffect, pub effect_digest:ActionDigest,
    pub execution_id:Uuid, pub source:ActionSourceFence, pub dependency:Option<ContextDependency>,
    pub review:ActionReviewRef, pub authorization:Option<ActionAuthorization>,
    pub created_at:DateTime<Utc>, pub expires_at:DateTime<Utc>, pub state:ActionState,
    pub execution:Option<ExecutionIntent>, pub collection:Option<CollectionTicket>,
}

impl ActionRecord {
    pub fn validate(&self)->Result<(),AgentFailure>{
        if self.id.is_nil() || self.execution_id.is_nil() || self.id==self.execution_id || !self.person_id.is_valid()
            || !bounded(&self.device_id,256) || self.revision==0 || self.expires_at<=self.created_at
            || self.expires_at-self.created_at>Duration::minutes(15) {return Err(AgentFailure::InvalidInput);}
        self.effect.validate(self.person_id)?;
        self.source.validate(&self.effect,&self.device_id)?;
        if self.effect.digest()?!=self.effect_digest || self.review.action_id!=self.id || self.review.id.is_nil()
            || self.review.effect_digest!=self.effect_digest || self.review.source_digest!=self.source.digest()?
            || self.review.authority_revision==0 || self.review.expires_at!=self.expires_at {return Err(AgentFailure::InvalidInput);}
        match (&self.origin,&self.dependency) {
            (ActionOrigin::Direct{command_id,actor_device_id},None) if !command_id.is_nil() && actor_device_id==&self.device_id=>{},
            (ActionOrigin::Expert{task_id,invocation_id,package,installation_id,assignment_id,definition_revision,artifact_id,..},Some(dependency))
                if !task_id.is_nil() && !invocation_id.is_nil() && !installation_id.is_nil() && !assignment_id.is_nil()
                && *definition_revision>0 && !artifact_id.is_nil() && package.kind==PackageKind::Expert
                && matches!(self.effect,CalendarEffect::Create{..}) && dependency.person_id()==self.person_id
                && dependency.source().connection_id()==&self.source.connection_id && dependency.source_authority()==self.source.authority
                && self.expires_at<=dependency.expires_at()=>{dependency.validate().map_err(|_|AgentFailure::InvalidInput)?;},
            _=>return Err(AgentFailure::PolicyDenied),
        }
        if let Some(intent)=&self.execution {
            if intent.action_id!=self.id || intent.person_id!=self.person_id || intent.device_id!=self.device_id
                || intent.execution_id!=self.execution_id || intent.effect_digest!=self.effect_digest || intent.effect!=self.effect
                || intent.source!=self.source || self.authorization.as_ref()!=Some(&intent.authorization) || intent.executor_generation==0
                || intent.prepared_at<self.created_at || intent.prepared_at>=self.expires_at {return Err(AgentFailure::InvalidInput);}
            match &self.state {
                ActionState::Executing{execution_id} if execution_id==&self.execution_id=>{},
                ActionState::Unknown{..}=>{},
                ActionState::Failed{reason,not_applied_proof} if reason==&not_applied_proof.reason=>CalendarEffectOutcome::NotApplied{proof:not_applied_proof.clone()}.validate_for(intent)?,
                ActionState::Succeeded{receipt,..}=>receipt.validate_for(intent)?,
                _=>return Err(AgentFailure::InvalidInput),
            }
        } else if matches!(self.state,ActionState::Executing{..}|ActionState::Unknown{..}|ActionState::Failed{..}|ActionState::Succeeded{..}) {
            return Err(AgentFailure::InvalidInput);
        }
        match (&self.state,&self.collection) {
            (ActionState::Succeeded{receipt,collection},Some(ticket)) if ticket.person_id==self.person_id && ticket.action_id==self.id
                && ticket.execution_id==self.execution_id && ticket.receipt_digest==receipt.digest()? && ticket.revision>0
                && !ticket.id.is_nil() && &ticket.state==collection=>{},
            (ActionState::Succeeded{..},_)|(_,Some(_))=>return Err(AgentFailure::InvalidInput),
            _=>{},
        }
        if let Some(authorization)=&self.authorization {authorization.validate_for(self,self.created_at)?;}
        action_digest(b"floe.actions.record.v1\0",self)?;
        Ok(())
    }
    pub fn pending_recovery(&self)->bool {matches!(self.state,ActionState::Executing{..}|ActionState::Unknown{..}|ActionState::Succeeded{collection:ActionCollectionState::Pending{..},..})}
}
