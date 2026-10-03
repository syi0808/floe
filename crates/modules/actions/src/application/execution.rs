use std::{collections::HashMap,sync::{Arc,Mutex,atomic::Ordering},time::Duration};
use floe_execution::{Cancellation,ExecutionScope};
use floe_kernel::AgentFailure;
use tokio::time::Instant;
use uuid::Uuid;
use crate::*;
use super::owner::day_failure;

impl ActionsService {
    pub(super) fn spawn(&self,id:Uuid,recovery:bool,parent:&ExecutionScope)->Result<(),AgentFailure>{
        if self.closed.load(Ordering::Acquire){return Err(AgentFailure::VaultUnavailable);}
        let cancellation=Cancellation::new();
        let mut jobs=self.jobs.lock().map_err(|_|AgentFailure::StorageUnavailable)?;
        if self.closed.load(Ordering::Acquire){return Err(AgentFailure::VaultUnavailable);}
        if jobs.contains_key(&id){return Ok(());}
        if jobs.len()>=64{return Err(AgentFailure::BudgetExceeded);}
        jobs.insert(id,cancellation.clone());
        drop(jobs);
        let scope=ExecutionScope::root(cancellation,Instant::now()+Duration::from_secs(45),parent.budget().child(0,0),parent.trace_context());
        let service=self.clone();
        let lease=ActionJobLease{id,jobs:self.jobs.clone()};
        tokio::spawn(async move {let _lease=lease;let _=if recovery {service.recover(id,&scope).await}else{service.drive(id,&scope).await};});
        Ok(())
    }
    async fn stop(&self,record:&ActionRecord,state:PreDispatchState)->Result<(),AgentFailure>{
        self.repository.stop_before_dispatch(PreDispatchStop{person_id:record.person_id,action_id:record.id,expected_revision:record.revision,state}).await?;
        Ok(())
    }
    async fn drive(&self,id:Uuid,scope:&ExecutionScope)->Result<(),AgentFailure>{
        let actor=&self.actor;
        let record=self.record(actor,id).await?;
        if record.state!=ActionState::Approved{return Ok(());}
        if self.closed.load(Ordering::Acquire)||scope.cancellation().is_cancelled(){return self.stop(&record,PreDispatchState::Cancelled).await;}
        if self.clock.now()>=record.expires_at{return self.stop(&record,PreDispatchState::Expired).await;}
        let source=match self.observe_source(actor,&record.effect).await {
            Ok(source) if source==record.source=>source,
            _=>return self.stop(&record,PreDispatchState::Blocked{reason:ActionBlockedReason::SourceChanged}).await,
        };
        let events=match self.current_events(actor,&record.effect,scope).await {
            Ok(events)=>events,Err(_)=>return self.stop(&record,PreDispatchState::Blocked{reason:ActionBlockedReason::SourceChanged}).await,
        };
        let prepared=match self.executor.prepare(actor,&record,&events,scope).await {
            Ok(prepared)=>prepared,Err(reason)=>return self.stop(&record,PreDispatchState::Blocked{reason}).await,
        };
        if self.closed.load(Ordering::Acquire)||scope.cancellation().is_cancelled(){return self.stop(&record,PreDispatchState::Cancelled).await;}
        if Instant::now()>=scope.deadline(){return self.stop(&record,PreDispatchState::Blocked{reason:ActionBlockedReason::ExecutorUnavailable}).await;}
        let unchanged_source=self.observe_source(actor,&record.effect).await.is_ok_and(|current|current==source);
        let unchanged_events=self.current_events(actor,&record.effect,scope).await.is_ok_and(|current|current==events);
        if !unchanged_source || !unchanged_events {
            return self.stop(&record,PreDispatchState::Blocked{reason:ActionBlockedReason::SourceChanged}).await;
        }
        if self.clock.now()>=record.expires_at{return self.stop(&record,PreDispatchState::Expired).await;}
        let admission=self.repository.prepare_dispatch(DispatchIntent{action_id:record.id,person_id:record.person_id,device_id:record.device_id.clone(),
            expected_revision:record.revision,execution_id:record.execution_id,effect_digest:record.effect_digest,
            authorization:record.authorization.clone().ok_or(AgentFailure::PolicyDenied)?,current_source_fence:source,
            executor_generation:prepared.executor_generation(),now:self.clock.now()}).await?;
        if !admission.dispatch_required{return Ok(());}
        let intent=admission.intent.clone();
        let revision=admission.record_revision;
        let outcome=prepared.dispatch(admission,scope.clone()).await;
        self.settle(&intent,revision,outcome,scope).await
    }
    async fn recover(&self,id:Uuid,scope:&ExecutionScope)->Result<(),AgentFailure>{
        let actor=&self.actor;
        self.admit_actor(actor,scope)?;
        let record=self.record(actor,id).await?;
        if matches!(record.state,ActionState::Succeeded{..}) {return self.collect(&record,scope).await;}
        if !matches!(record.state,ActionState::Executing{..}|ActionState::Unknown{..}){return Err(AgentFailure::Conflict);}
        let admission=self.repository.load_execution(actor.person_id,record.execution_id).await?.ok_or(AgentFailure::StorageUnavailable)?;
        if admission.dispatch_required || record.execution.as_ref()!=Some(&admission.intent){return Err(AgentFailure::StorageUnavailable);}
        let outcome=self.executor.recover(actor,&admission.intent,scope).await;
        self.settle(&admission.intent,record.revision,outcome,scope).await
    }
    async fn settle(&self,intent:&ExecutionIntent,revision:u64,outcome:CalendarEffectOutcome,scope:&ExecutionScope)->Result<(),AgentFailure>{
        let outcome=if outcome.validate_for(intent).is_ok(){outcome}else{CalendarEffectOutcome::Unknown{identity:intent.identity(),reason:ActionUnknownReason::InvalidReceipt}};
        let record=self.repository.settle_execution(ExecutionSettlement{person_id:intent.person_id,execution_id:intent.execution_id,effect_digest:intent.effect_digest,
            expected_revision:revision,outcome}).await?;
        if matches!(record.state,ActionState::Succeeded{..}){self.collect(&record,scope).await?;}
        Ok(())
    }
    async fn collect(&self,record:&ActionRecord,scope:&ExecutionScope)->Result<(),AgentFailure>{
        let ActionState::Succeeded{receipt,collection:ActionCollectionState::Pending{..}}=&record.state else{return Ok(())};
        let ticket=record.collection.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
        let destination=record.effect.destination();
        let source=floe_day::ActionCollectionSource{connection_id:destination.connection_id.clone(),connection_revision:destination.connection_revision,
            provider:destination.provider,calendar_id:destination.calendar_id.clone(),calendar_name:destination.calendar_name.clone()};
        let calendar_record=|event:&CalendarWriteResult|floe_day::CalendarRecord{can_modify:event.can_modify,calendar_id:destination.calendar_id.clone(),external_id:event.external_id.clone(),
            external_revision:event.external_revision.clone(),title:event.title.clone(),schedule:floe_day::EventSchedule::Timed(event.schedule.clone())};
        let collection=match &receipt.effect {
            CommittedCalendarEffect::Created{event}=>floe_day::CalendarActionCollection::Created{source,record:calendar_record(event)},
            CommittedCalendarEffect::Updated{target,event}=>{let original=target.source()?;floe_day::CalendarActionCollection::Updated{source,
                expected_external_id:original.external_id.clone(),expected_external_revision:original.external_revision.clone(),record:calendar_record(event)}},
            CommittedCalendarEffect::Deleted{target}=>{let original=target.source()?;floe_day::CalendarActionCollection::Deleted{source,
                external_id:original.external_id.clone(),expected_external_revision:original.external_revision.clone()}},
        };
        let receipt_digest=receipt.digest()?;
        let result=self.day.collect_action(&self.actor,record.execution_id,receipt_digest,collection,scope).await.map_err(day_failure)?;
        if result.execution_id!=record.execution_id || result.receipt_digest!=receipt_digest{return Err(AgentFailure::Conflict);}
        self.repository.ack_collection(CollectionAck{person_id:record.person_id,execution_id:record.execution_id,receipt_digest,
            expected_ticket_revision:ticket.revision,day_projection_ref:result.day_projection_ref}).await?;
        Ok(())
    }
}

struct ActionJobLease{id:Uuid,jobs:Arc<Mutex<HashMap<Uuid,Cancellation>>>}
impl Drop for ActionJobLease{fn drop(&mut self){if let Ok(mut jobs)=self.jobs.lock(){jobs.remove(&self.id);}}}
