use std::{collections::HashMap,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::Duration};
use floe_day::{DayService,Event};
use floe_execution::{Cancellation,ExecutionScope};
use floe_kernel::{AgentFailure,OwnerActor};
use tokio::time::Instant;
use uuid::Uuid;
use crate::*;

pub struct ActionsDependencies {
    pub repository:Arc<dyn ActionsRepository>,pub sources:Arc<dyn ActionSourceReader>,
    pub proposals:Arc<dyn ExpertProposalReader>,pub day:Arc<DayService>,
    pub executor:Arc<dyn ActionCalendarExecutor>,pub clock:Arc<dyn ActionsClock>,
}

#[derive(Clone)]
pub struct ActionsService {
    pub(super) actor:OwnerActor,
    pub(super) repository:Arc<dyn ActionsRepository>,pub(super) sources:Arc<dyn ActionSourceReader>,pub(super) proposals:Arc<dyn ExpertProposalReader>,
    pub(super) day:Arc<DayService>,pub(super) executor:Arc<dyn ActionCalendarExecutor>,pub(super) clock:Arc<dyn ActionsClock>,
    pub(super) closed:Arc<AtomicBool>,pub(super) jobs:Arc<Mutex<HashMap<Uuid,Cancellation>>>,
}

impl ActionsService {
    pub fn new(actor:OwnerActor,dependencies:ActionsDependencies)->Result<Self,AgentFailure>{
        actor.validate()?;
        Ok(Self{actor,repository:dependencies.repository,sources:dependencies.sources,proposals:dependencies.proposals,
            day:dependencies.day,executor:dependencies.executor,clock:dependencies.clock,closed:Arc::new(AtomicBool::new(false)),jobs:Arc::new(Mutex::new(HashMap::new()))})
    }
    pub(super) fn admit_actor(&self,actor:&OwnerActor,scope:&ExecutionScope)->Result<(),AgentFailure>{
        actor.validate()?;
        if self.closed.load(Ordering::Acquire) || actor!=&self.actor {return Err(AgentFailure::VaultUnavailable);}
        if scope.cancellation().is_cancelled(){return Err(AgentFailure::Cancelled);}
        if Instant::now()>=scope.deadline(){return Err(AgentFailure::DeadlineExceeded);}
        Ok(())
    }
    pub fn shutdown(&self){
        self.closed.store(true,Ordering::Release);
        if let Ok(jobs)=self.jobs.lock(){for cancellation in jobs.values(){cancellation.cancel();}}
    }
    pub async fn shutdown_and_drain(&self,scope:&ExecutionScope)->Result<(),AgentFailure>{
        self.shutdown();
        loop {
            if self.jobs.lock().map_err(|_|AgentFailure::StorageUnavailable)?.is_empty(){return Ok(());}
            if scope.cancellation().is_cancelled(){return Err(AgentFailure::Cancelled);}
            if Instant::now()>=scope.deadline(){return Err(AgentFailure::DeadlineExceeded);}
            tokio::select!{_=tokio::time::sleep_until(scope.deadline())=>return Err(AgentFailure::DeadlineExceeded),_=tokio::time::sleep(Duration::from_millis(10))=>{}}
        }
    }
    pub async fn inspect(&self,actor:&OwnerActor,action_ref:Uuid,scope:&ExecutionScope)->Result<ActionSnapshot,AgentFailure>{
        self.admit_actor(actor,scope)?;
        let record=self.record(actor,action_ref).await?;
        ActionSnapshot::from_record(&record,self.clock.now())
    }
    pub async fn list(&self,actor:&OwnerActor,cursor:Option<Uuid>,limit:u16,scope:&ExecutionScope)->Result<ActionsPage,AgentFailure>{
        self.admit_actor(actor,scope)?;
        if !(1..=100).contains(&limit){return Err(AgentFailure::InvalidInput);}
        let page=self.repository.list(actor.person_id,cursor,limit).await?;
        let actions=page.records.iter().map(|record|{self.validate_record_actor(actor,record)?;ActionSnapshot::from_record(record,self.clock.now())}).collect::<Result<_,_>>()?;
        Ok(ActionsPage{actions,next_cursor:page.next_cursor})
    }
    pub async fn inspect_authority(&self,actor:&OwnerActor,scope:&ExecutionScope)->Result<ActionsAuthority,AgentFailure>{
        self.admit_actor(actor,scope)?;
        Ok(self.repository.read_authority(actor.person_id).await?)
    }
    pub async fn set_calendar_create_authority(&self,actor:&OwnerActor,command_id:Uuid,mode:ActionAuthorityMode,expected_revision:u64,scope:&ExecutionScope)->Result<ActionsAuthority,AgentFailure>{
        self.admit_actor(actor,scope)?;
        if command_id.is_nil() || expected_revision==0{return Err(AgentFailure::InvalidInput);}
        Ok(self.repository.compare_and_set_authority(AuthorityChange{command_id,person_id:actor.person_id,expected_revision,mode}).await?)
    }
    pub async fn decide(&self,actor:&OwnerActor,command_id:Uuid,action_ref:Uuid,review_ref:ActionReviewRef,decision:ActionDecisionKind,expected_revision:u64,scope:&ExecutionScope)->Result<ActionSnapshot,AgentFailure>{
        self.admit_actor(actor,scope)?;
        let record=self.repository.record_decision(ActionDecision{command_id,person_id:actor.person_id,device_id:actor.device_id.clone(),action_id:action_ref,
            expected_revision,review_ref,decision,now:self.clock.now()}).await?;
        self.validate_record_actor(actor,&record)?;
        if record.state==ActionState::Approved {self.spawn(record.id,false,scope)?;}
        ActionSnapshot::from_record(&record,self.clock.now())
    }
    pub async fn reconcile(&self,actor:&OwnerActor,command_id:Uuid,action_ref:Uuid,expected_revision:u64,scope:&ExecutionScope)->Result<ActionSnapshot,AgentFailure>{
        self.admit_actor(actor,scope)?;
        let record=self.repository.admit_reconciliation(ActionReconciliation{command_id,person_id:actor.person_id,device_id:actor.device_id.clone(),action_id:action_ref,expected_revision}).await?;
        self.validate_record_actor(actor,&record)?;
        if record.pending_recovery(){self.spawn(record.id,true,scope)?;}
        ActionSnapshot::from_record(&record,self.clock.now())
    }
    /// Host activation never dispatches Approved work. Recovery is explicit and lookup-only.
    pub async fn activate(&self,actor:&OwnerActor,scope:&ExecutionScope)->Result<(),AgentFailure>{
        self.admit_actor(actor,scope)?;
        self.repository.read_authority(actor.person_id).await?;
        Ok(())
    }
    pub(super) fn validate_record_actor(&self,actor:&OwnerActor,record:&ActionRecord)->Result<(),AgentFailure>{
        record.validate()?;
        if record.person_id!=actor.person_id || record.device_id!=actor.device_id{return Err(AgentFailure::NotFound);}
        Ok(())
    }
    pub(super) async fn record(&self,actor:&OwnerActor,id:Uuid)->Result<ActionRecord,AgentFailure>{
        let record=self.repository.get(actor.person_id,id).await?.ok_or(AgentFailure::NotFound)?;
        self.validate_record_actor(actor,&record)?;
        Ok(record)
    }
    pub(super) async fn observe_source(&self,actor:&OwnerActor,effect:&CalendarEffect)->Result<ActionSourceFence,AgentFailure>{
        let destination=effect.destination();
        if self.sources.source_is_fenced(actor.person_id,&destination.connection_id).await? {return Err(AgentFailure::PolicyDenied);}
        let source=self.sources.load(actor.person_id,&destination.connection_id).await?.ok_or(AgentFailure::NotFound)?;
        if source.person_id()!=actor.person_id || source.connector_id().as_str()!="calendar.event_kit" || !source.is_serving()
            || source.revision()!=destination.connection_revision || !source.resources().iter().any(|resource|resource.handle().as_str()==destination.calendar_id && resource.label()==destination.calendar_name) {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut resources:Vec<_>=source.resources().iter().map(|resource|resource.handle().as_str().to_owned()).collect();
        resources.sort();
        let fence=ActionSourceFence{connection_id:source.connection_id().clone(),revision:source.revision(),authority:source.source_authority(),
            execution_owner:source.execution_owner_id().as_str().to_owned(),resources,
            native_subject_fingerprint:source.native_subject_fingerprint().ok_or(AgentFailure::PolicyDenied)?.to_owned()};
        fence.validate(effect,&actor.device_id)?;
        Ok(fence)
    }
    pub(super) async fn current_events(&self,actor:&OwnerActor,effect:&CalendarEffect,scope:&ExecutionScope)->Result<Vec<Event>,AgentFailure>{
        let mirror=self.day.calendar_mirror(actor,scope).await.map_err(day_failure)?;
        if let Some(target)=effect.target(){
            let mirror=mirror.as_ref().ok_or(AgentFailure::Conflict)?;
            if !mirror.events.contains(&target.original){return Err(AgentFailure::Conflict);}
        }
        if let Some(mirror)=&mirror {
            if mirror.state.source_state(&effect.destination().connection_id).is_some_and(|state|state.error.is_some()
                || state.calendar_statuses.get(&effect.destination().calendar_id).is_some_and(|status|status.error.is_some())) {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(mirror.map(|mirror|mirror.events).unwrap_or_default())
    }
}

pub(super) fn day_failure(error:floe_day::DayError)->AgentFailure{match error.code {floe_day::DayErrorCode::Validation=>AgentFailure::InvalidInput,
    floe_day::DayErrorCode::NotFound=>AgentFailure::NotFound,floe_day::DayErrorCode::Conflict=>AgentFailure::Conflict,_=>AgentFailure::StorageUnavailable}}
