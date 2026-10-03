use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use floe_access::{CalendarReadAccessRequest, CalendarReadAccessStamp};
use floe_actions::{ActionCalendarExecutor, PreparedCalendarEffect, ActionRecord, ActionSourceFence, CalendarEffect,
    ActionBlockedReason, ActionUnknownReason, EffectIdentity, ExecutionIntent, DispatchAdmission,
    CalendarEffectOutcome, CalendarReceiptEvidence, CalendarDestinationObservation};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::OwnerActor;
use floe_agent_contract::AgentFailure;
use floe_context::{CalendarObservation, CalendarObserveRequest, CalendarSource};
use floe_execution::Cancellation;

use floe_agent_contract::PersonId;
use floe_context_contract::CalendarProvider;
use floe_day::{
    AllDaySchedule, CalendarBatch, CalendarFailure, CalendarRecord, Event, EventSchedule,
    TimedSchedule,
};
use floe_native::{
    NATIVE_CALENDAR_WIRE_VERSION, NativeCalendarBatch as CalendarBatchDto,
    NativeCalendarFailure as CalendarFailureDto, NativeEventSchedule as EventScheduleDto,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum NativeReadFailure { PermissionDenied, ProviderUnavailable, Timeout, UncertainResult }

fn schedule_from_native(value: EventScheduleDto) -> Result<EventSchedule, AgentFailure> {
    match value {
        EventScheduleDto::Timed {
            starts_at,
            ends_at,
            timezone,
        } => {
            let starts_at = DateTime::parse_from_rfc3339(&starts_at)
                .map_err(|_| AgentFailure::InvalidInput)?
                .with_timezone(&Utc);
            let ends_at = DateTime::parse_from_rfc3339(&ends_at)
                .map_err(|_| AgentFailure::InvalidInput)?
                .with_timezone(&Utc);
            TimedSchedule::new(starts_at, ends_at, timezone)
                .map(EventSchedule::Timed)
                .map_err(|_| AgentFailure::InvalidInput)
        }
        EventScheduleDto::AllDay {
            start_date,
            end_date_exclusive,
        } => {
            let start_date = NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
                .map_err(|_| AgentFailure::InvalidInput)?;
            let end_date_exclusive = NaiveDate::parse_from_str(&end_date_exclusive, "%Y-%m-%d")
                .map_err(|_| AgentFailure::InvalidInput)?;
            AllDaySchedule::new(start_date, end_date_exclusive)
                .map(EventSchedule::AllDay)
                .map_err(|_| AgentFailure::InvalidInput)
        }
    }
}

fn failure_from_native(value: CalendarFailureDto) -> CalendarFailure {
    match value {
        CalendarFailureDto::PermissionDenied => CalendarFailure::PermissionDenied,
        CalendarFailureDto::CalendarUnavailable => CalendarFailure::CalendarUnavailable,
        CalendarFailureDto::ProviderUnavailable => CalendarFailure::ProviderUnavailable,
    }
}

pub struct NativeCalendarExecutor {
    actor: OwnerActor,
    sources: Arc<dyn floe_connections::ConnectionsRepository>,
}

pub struct NativeCalendarReadAccess {
    person_id: PersonId,
    device_id: String,
    provider: CalendarProvider,
    calendar_ids: Vec<String>,
    connection_id: String,
    connection_revision: u64,
    sources: Arc<dyn floe_connections::ConnectionsRepository>,
}

impl NativeCalendarReadAccess {
    pub fn new(
        person_id: PersonId,
        device_id: String,
        provider: CalendarProvider,
        calendar_ids: Vec<String>,
        connection_id: String,
        connection_revision: u64,
        sources: Arc<dyn floe_connections::ConnectionsRepository>,
    ) -> Self {
        let mut calendar_ids = calendar_ids;
        calendar_ids.sort();
        Self {
            person_id,
            device_id,
            provider,
            calendar_ids,
            connection_id,
            connection_revision,
            sources,
        }
    }

    async fn require_unfenced(&self) -> Result<(), AgentFailure> {
        let id = floe_context_contract::ConnectionId::try_new(self.connection_id.clone())
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if self
            .sources
            .source_is_fenced(self.person_id, &id)
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let current = self
            .sources
            .load(self.person_id, &id)
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        if current.revision() != self.connection_revision || !current.is_serving() {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    fn validate_request(
        &self,
        person_id: PersonId,
        device_id: &str,
        provider: CalendarProvider,
        calendar_ids: &[String],
    ) -> Result<(), AgentFailure> {
        if provider != CalendarProvider::EventKit {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if person_id != self.person_id
            || !self.person_id.is_valid()
            || device_id != self.device_id
            || provider != self.provider
            || self.connection_id.trim().is_empty()
            || self.connection_revision == 0
            || self.device_id.len() > 128
            || self.device_id.chars().any(char::is_control)
            || self.connection_id.len() > 128
            || self.connection_id.chars().any(char::is_control)
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut expected = self.calendar_ids.clone();
        let mut actual = calendar_ids.to_vec();
        expected.sort();
        actual.sort();
        if actual != expected
            || actual.len() != expected.len()
            || actual.iter().collect::<HashSet<_>>().len() != actual.len()
            || expected.iter().collect::<HashSet<_>>().len() != expected.len()
            || actual.iter().any(|id| {
                id.trim().is_empty() || id.len() > 512 || id.chars().any(char::is_control)
            })
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        if actual.is_empty() || actual.iter().any(|id| id.trim().is_empty()) {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }

    fn request_base(&self, operation: &str, deadline: Instant) -> Result<Value, AgentFailure> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let deadline = Utc::now()
            .checked_add_signed(
                chrono::Duration::from_std(remaining.min(Duration::from_secs(30)))
                    .map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?;
        Ok(json!({
            "operation": operation,
            "schema_version": NATIVE_CALENDAR_WIRE_VERSION,
            "person_id": self.person_id,
            "device_id": self.device_id,
            "provider": self.provider,
            "connection_id": self.connection_id,
            "connection_revision": self.connection_revision,
            "calendar_ids": self.calendar_ids,
            "item_limit": 128,
            "byte_limit": 65_536,
            "deadline": deadline.to_rfc3339(),
        }))
    }

    async fn native<T: DeserializeOwned + Send + 'static>(
        request: Value,
        deadline: Instant,
        cancellation: Cancellation,
    ) -> Result<T, AgentFailure> {
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        static READ_ADMISSION: OnceLock<Arc<Semaphore>> = OnceLock::new();
        let permit = READ_ADMISSION
            .get_or_init(|| Arc::new(Semaphore::new(1)))
            .clone()
            .try_acquire_owned()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            call_read(request)
        });
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
            result = task => result.map_err(|_| AgentFailure::Interrupted)?,
        };
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        result.map_err(native_read_failure)
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeObservation {
    stamp: CalendarReadAccessStamp,
    observed_at: DateTime<Utc>,
    batches: Vec<CalendarBatchDto>,
}

impl CalendarSource for NativeCalendarReadAccess {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        self.require_unfenced().await?;
        self.validate_request(
            request.person_id,
            &request.device_id,
            request.provider,
            &request.calendar_ids,
        )?;
        let input = self.request_base("view_access", request.deadline)?;
        let stamp: CalendarReadAccessStamp =
            Self::native(input, request.deadline, request.cancellation).await?;
        if stamp.schema_version != NATIVE_CALENDAR_WIRE_VERSION
            || stamp.person_id != self.person_id
            || stamp.device_id != self.device_id
            || stamp.provider != CalendarProvider::EventKit
            || stamp.calendar_ids != self.calendar_ids
            || stamp.native_subject_fingerprint.len() != 64
            || stamp
                .native_subject_fingerprint
                .bytes()
                .any(|byte| !byte.is_ascii_hexdigit())
            || stamp.generation.trim().is_empty()
            || stamp.generation.len() > 128
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        self.require_unfenced().await?;
        Ok(stamp)
    }

    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        self.validate_request(
            request.person_id,
            &request.device_id,
            request.provider,
            &request.calendar_ids,
        )?;
        if request.starts_at >= request.ends_at
            || request.ends_at - request.starts_at > chrono::Duration::days(32)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let before = self
            .check(CalendarReadAccessRequest {
                person_id: request.person_id,
                device_id: request.device_id.clone(),
                provider: request.provider,
                calendar_ids: request.calendar_ids.clone(),
                expected_native_subject_fingerprint: None,
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            })
            .await?;
        if let Some(expected) = request.expected_native_subject_fingerprint.as_deref()
            && expected != before.native_subject_fingerprint
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let cancellation = request.cancellation.clone();
        let mut input = self.request_base("observe", request.deadline)?;
        if let Some(expected) = request.expected_native_subject_fingerprint.as_deref() {
            input["expected_native_subject_fingerprint"] = json!(expected);
        }
        input["starts_at"] = json!(request.starts_at.to_rfc3339());
        input["ends_at"] = json!(request.ends_at.to_rfc3339());
        let observation: NativeObservation =
            Self::native(input, request.deadline, cancellation.clone()).await?;
        let after = self
            .check(CalendarReadAccessRequest {
                person_id: request.person_id,
                device_id: request.device_id,
                provider: request.provider,
                calendar_ids: request.calendar_ids,
                expected_native_subject_fingerprint: None,
                deadline: request.deadline,
                cancellation: cancellation.clone(),
            })
            .await?;
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= request.deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        if observation.stamp != before || before != after {
            return Err(AgentFailure::StaleContext);
        }
        if observation.observed_at > Utc::now()
            || Utc::now() - observation.observed_at > chrono::Duration::minutes(5)
        {
            return Err(AgentFailure::StaleContext);
        }
        let expected: HashSet<_> = self.calendar_ids.iter().map(String::as_str).collect();
        if observation.batches.len() != expected.len()
            || observation
                .batches
                .iter()
                .map(|batch| batch.calendar_id.as_str())
                .collect::<HashSet<_>>()
                != expected
            || observation
                .batches
                .iter()
                .any(|batch| batch.failure.is_some())
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let raw_count = observation
            .batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>();
        if raw_count > 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut record_ids = HashSet::new();
        if observation.batches.iter().any(|batch| {
            batch.records.iter().any(|record| {
                record.calendar_id != batch.calendar_id
                    || record.external_id.trim().is_empty()
                    || record.external_id.len() > 512
                    || record.external_revision.trim().is_empty()
                    || record.external_revision.len() > 512
                    || record.title.len() > 4096
                    || !record_ids.insert((batch.calendar_id.as_str(), record.external_id.as_str()))
            })
        }) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let batches = observation
            .batches
            .into_iter()
            .map(|batch| {
                let records = batch
                    .records
                    .into_iter()
                    .map(|record| {
                        Ok(CalendarRecord {
                            can_modify: record.can_modify,
                            calendar_id: record.calendar_id,
                            external_id: record.external_id,
                            external_revision: floe_day::CalendarExternalRevision::from_observation_fingerprint_hex(&record.external_revision).ok_or(AgentFailure::InvalidInput)?,
                            title: record.title,
                            schedule: schedule_from_native(record.schedule)?,
                        })
                    })
                    .collect::<Result<Vec<_>, AgentFailure>>()?;
                Ok(CalendarBatch {
                    calendar_id: batch.calendar_id,
                    records,
                    failure: batch.failure.map(failure_from_native),
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        let count = batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>();
        if count > 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= request.deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        Ok(Some(CalendarObservation {
            stamp: before,
            observed_at: observation.observed_at,
            batches,
        }))
    }
}

fn native_failure(failure: NativeReadFailure) -> AgentFailure {
    match failure {
        NativeReadFailure::PermissionDenied => AgentFailure::CapabilityDenied,
        NativeReadFailure::ProviderUnavailable => AgentFailure::CapabilityUnavailable,
        NativeReadFailure::Timeout => AgentFailure::DeadlineExceeded,
        NativeReadFailure::UncertainResult => AgentFailure::CapabilityUnavailable,
    }
}

enum ReadCallFailure {
    Action(NativeReadFailure),
    BudgetExceeded,
}

fn native_read_failure(failure: ReadCallFailure) -> AgentFailure {
    match failure {
        ReadCallFailure::BudgetExceeded => AgentFailure::BudgetExceeded,
        ReadCallFailure::Action(NativeReadFailure::PermissionDenied) => {
            AgentFailure::AccessReviewRequired
        }
        ReadCallFailure::Action(NativeReadFailure::UncertainResult) => {
            AgentFailure::CapabilityUnavailable
        }
        ReadCallFailure::Action(other) => native_failure(other),
    }
}

impl NativeCalendarExecutor {
    pub fn new(actor:OwnerActor,sources:Arc<dyn floe_connections::ConnectionsRepository>)->Result<Self,AgentFailure>{
        actor.validate()?;
        Ok(Self{actor,sources})
    }
}

#[derive(serde::Deserialize,serde::Serialize)]
#[serde(tag="status",rename_all="snake_case",deny_unknown_fields)]
enum NativeActionPreparation {
    Ready{schema_version:u32,identity:EffectIdentity,host_epoch:uuid::Uuid,preparation_id:uuid::Uuid,native_subject_fingerprint:String,expires_at:DateTime<Utc>},
    Blocked{schema_version:u32,identity:EffectIdentity,reason:ActionBlockedReason},
}

struct NativePreparedCalendarEffect {
    actor:OwnerActor,
    sources:Arc<dyn floe_connections::ConnectionsRepository>,
    record:ActionRecord,
    identity:EffectIdentity,
    host_epoch:uuid::Uuid,
    preparation_id:uuid::Uuid,
    expires_at:DateTime<Utc>,
}

fn action_identity(actor:&OwnerActor,record:&ActionRecord)->EffectIdentity{
    EffectIdentity{execution_id:record.execution_id,effect_digest:record.effect_digest,person_id:actor.person_id,device_id:actor.device_id.clone(),
        executor_generation:actor.runtime_epoch,connection_id:record.effect.destination().connection_id.clone(),calendar_id:record.effect.destination().calendar_id.clone()}
}

async fn require_action_source(sources:&dyn floe_connections::ConnectionsRepository,actor:&OwnerActor,effect:&CalendarEffect,expected:&ActionSourceFence)->Result<(),AgentFailure>{
    expected.validate(effect,&actor.device_id)?;
    require_source_fence(sources,actor,expected).await
}

async fn require_source_fence(sources:&dyn floe_connections::ConnectionsRepository,actor:&OwnerActor,expected:&ActionSourceFence)->Result<(),AgentFailure>{
    expected.validate_identity(&actor.device_id)?;
    if sources.source_is_fenced(actor.person_id,&expected.connection_id).await.map_err(|_|AgentFailure::PolicyDenied)?{return Err(AgentFailure::PolicyDenied);}
    let current=sources.load(actor.person_id,&expected.connection_id).await.map_err(|_|AgentFailure::PolicyDenied)?.ok_or(AgentFailure::PolicyDenied)?;
    let mut resources:Vec<_>=current.resources().iter().map(|resource|resource.handle().as_str().to_owned()).collect();
    resources.sort();
    if current.person_id()!=actor.person_id || !current.is_serving() || current.connector_id().as_str()!="calendar.event_kit"
        || current.revision()!=expected.revision || current.source_authority()!=expected.authority
        || current.execution_owner_id().as_str()!=expected.execution_owner || current.native_subject_fingerprint()!=Some(expected.native_subject_fingerprint.as_str())
        || resources!=expected.resources {return Err(AgentFailure::PolicyDenied);}
    Ok(())
}

fn native_action_deadline(scope:&ExecutionScope)->Result<DateTime<Utc>,AgentFailure>{
    if scope.cancellation().is_cancelled(){return Err(AgentFailure::Cancelled);}
    let remaining=scope.deadline().checked_duration_since(Instant::now()).ok_or(AgentFailure::DeadlineExceeded)?.min(Duration::from_secs(12));
    Ok(Utc::now()+chrono::Duration::from_std(remaining).map_err(|_|AgentFailure::InvalidInput)?)
}

fn unknown(intent:&ExecutionIntent,reason:ActionUnknownReason)->CalendarEffectOutcome{CalendarEffectOutcome::Unknown{identity:intent.identity(),reason}}

impl ActionCalendarExecutor for NativeCalendarExecutor {
    fn destinations<'a>(&'a self,actor:&'a OwnerActor,source:&'a ActionSourceFence,scope:&'a ExecutionScope)
        ->BoxFuture<'a,Result<Vec<CalendarDestinationObservation>,ActionBlockedReason>>{
        Box::pin(async move{
            if actor!=&self.actor{return Err(ActionBlockedReason::PolicyDenied);}
            require_source_fence(self.sources.as_ref(),actor,source).await.map_err(|_|ActionBlockedReason::SourceChanged)?;
            let deadline=native_action_deadline(scope).map_err(|_|ActionBlockedReason::ExecutorUnavailable)?;
            let request=json!({"schema_version":1,"operation":"action_destinations","person_id":actor.person_id,"device_id":actor.device_id,
                "executor_generation":actor.runtime_epoch,"source":source,"deadline":deadline});
            let result:NativeActionDestinations=action_native(request,scope,false).await.map_err(|_|ActionBlockedReason::ExecutorUnavailable)?;
            if result.schema_version!=1 || result.person_id!=actor.person_id || result.device_id!=actor.device_id
                || result.executor_generation!=actor.runtime_epoch || result.source!=*source || result.resources.len()!=source.resources.len()
                || result.resources.iter().map(|item|item.calendar_id.as_str()).collect::<HashSet<_>>().len()!=result.resources.len()
                || result.resources.iter().any(|item|!source.resources.contains(&item.calendar_id)||item.calendar_name.is_empty()||item.calendar_name.len()>512
                    ||item.calendar_name.chars().any(char::is_control)) {return Err(ActionBlockedReason::ExecutorUnavailable);}
            require_source_fence(self.sources.as_ref(),actor,source).await.map_err(|_|ActionBlockedReason::SourceChanged)?;
            Ok(result.resources)
        })
    }
    fn prepare<'a>(&'a self,actor:&'a OwnerActor,record:&'a ActionRecord,local_events:&'a [Event],scope:&'a ExecutionScope)
        ->BoxFuture<'a,Result<Box<dyn PreparedCalendarEffect>,ActionBlockedReason>>{
        Box::pin(async move{
            if actor!=&self.actor || record.person_id!=actor.person_id || record.device_id!=actor.device_id || record.validate().is_err()
                || record.state!=floe_actions::ActionState::Approved || record.execution.is_some(){return Err(ActionBlockedReason::PolicyDenied);}
            require_action_source(self.sources.as_ref(),actor,&record.effect,&record.source).await.map_err(|_|ActionBlockedReason::SourceChanged)?;
            let identity=action_identity(actor,record);
            let deadline=native_action_deadline(scope).map_err(|_|ActionBlockedReason::ExecutorUnavailable)?;
            let request=json!({"schema_version":1,"operation":"action_preflight","identity":identity,"effect":record.effect,
                "source":record.source,"authorization_expires_at":record.expires_at,"local_events":local_events,"deadline":deadline});
            let response:NativeActionPreparation=action_native(request,scope,false).await.map_err(|_|ActionBlockedReason::ExecutorUnavailable)?;
            let (host_epoch,preparation_id,expires_at)=match response {
                NativeActionPreparation::Ready{schema_version,identity:actual,host_epoch,preparation_id,native_subject_fingerprint,expires_at}
                    if schema_version==1 && actual==identity && !host_epoch.is_nil() && !preparation_id.is_nil()
                    && native_subject_fingerprint==record.source.native_subject_fingerprint && expires_at>Utc::now()
                    && expires_at<=record.expires_at && expires_at<=Utc::now()+chrono::Duration::seconds(30)=>(host_epoch,preparation_id,expires_at),
                NativeActionPreparation::Blocked{schema_version:1,identity:actual,reason} if actual==identity=>return Err(reason),
                _=>return Err(ActionBlockedReason::ExecutorUnavailable),
            };
            require_action_source(self.sources.as_ref(),actor,&record.effect,&record.source).await.map_err(|_|ActionBlockedReason::SourceChanged)?;
            Ok(Box::new(NativePreparedCalendarEffect{actor:actor.clone(),sources:self.sources.clone(),record:record.clone(),identity,host_epoch,preparation_id,expires_at}) as Box<dyn PreparedCalendarEffect>)
        })
    }
    fn recover<'a>(&'a self,actor:&'a OwnerActor,intent:&'a ExecutionIntent,scope:&'a ExecutionScope)->BoxFuture<'a,CalendarEffectOutcome>{
        Box::pin(async move{
            if actor!=&self.actor || intent.person_id!=actor.person_id || intent.device_id!=actor.device_id {
                return unknown(intent,ActionUnknownReason::InvalidReceipt);
            }
            let Ok(deadline)=native_action_deadline(scope) else{return unknown(intent,ActionUnknownReason::Timeout)};
            let request=json!({"schema_version":1,"operation":"action_readback","admission":intent,"deadline":deadline});
            let readback:CalendarEffectOutcome=match action_native(request,scope,true).await {
                Ok(value)=>value,Err(_)=>return unknown(intent,ActionUnknownReason::NativeReceiptUnavailable),
            };
            if readback.validate_for(intent).is_err(){return unknown(intent,ActionUnknownReason::InvalidReceipt);}
            match &readback {
                CalendarEffectOutcome::Unknown{reason:ActionUnknownReason::NativeReceiptUnavailable,..}=>{},
                _=>return readback,
            }
            if !matches!(intent.effect,CalendarEffect::Create{..}){return readback;}
            if require_action_source(self.sources.as_ref(),actor,&intent.effect,&intent.source).await.is_err(){return unknown(intent,ActionUnknownReason::InconclusiveLookup);}
            let Ok(deadline)=native_action_deadline(scope) else{return unknown(intent,ActionUnknownReason::Timeout)};
            let request=json!({"schema_version":1,"operation":"action_lookup","admission":intent,"deadline":deadline});
            let result:CalendarEffectOutcome=match action_native(request,scope,false).await{
                Ok(value)=>value,Err(_)=>return unknown(intent,ActionUnknownReason::InconclusiveLookup),
            };
            if result.validate_for(intent).is_err(){return unknown(intent,ActionUnknownReason::InvalidReceipt);}
            match result {
                CalendarEffectOutcome::Committed{ref receipt} if matches!(receipt.evidence,CalendarReceiptEvidence::UniqueCreateMarker{..})=>result,
                CalendarEffectOutcome::Unknown{..}=>result,
                _=>unknown(intent,ActionUnknownReason::InvalidReceipt),
            }
        })
    }
}

impl PreparedCalendarEffect for NativePreparedCalendarEffect {
    fn executor_generation(&self)->u64{self.actor.runtime_epoch}
    fn dispatch(self:Box<Self>,admission:DispatchAdmission,scope:ExecutionScope)->BoxFuture<'static,CalendarEffectOutcome>{
        Box::pin(async move{
            let intent=&admission.intent;
            if !admission.dispatch_required || intent.identity()!=self.identity || intent.effect!=self.record.effect || intent.source!=self.record.source
                || self.record.authorization.as_ref()!=Some(&intent.authorization) || intent.action_id!=self.record.id
                || intent.prepared_at>=self.record.expires_at || Utc::now()>=self.expires_at {
                return unknown(intent,ActionUnknownReason::InvalidReceipt);
            }
            if scope.cancellation().is_cancelled(){return unknown(intent,ActionUnknownReason::CancelledAfterDispatch);}
            if require_action_source(self.sources.as_ref(),&self.actor,&intent.effect,&intent.source).await.is_err(){return unknown(intent,ActionUnknownReason::InconclusiveLookup);}
            let Ok(deadline)=native_action_deadline(&scope) else{return unknown(intent,ActionUnknownReason::Timeout)};
            let request=json!({"schema_version":1,"operation":"action_dispatch","admission":intent,"preparation_id":self.preparation_id,"host_epoch":self.host_epoch,"deadline":deadline});
            let result:CalendarEffectOutcome=match action_native(request,&scope,false).await{
                Ok(value)=>value,
                Err(NativeActionTransportError::NotInvoked(reason))=>return CalendarEffectOutcome::NotApplied{proof:floe_actions::NotAppliedProof{
                    identity:intent.identity(),host_epoch:self.host_epoch,invocation_id:self.preparation_id,reason,rejected_at:Utc::now()}},
                Err(NativeActionTransportError::Unknown(AgentFailure::Cancelled))=>return unknown(intent,ActionUnknownReason::CancelledAfterDispatch),
                Err(NativeActionTransportError::Unknown(AgentFailure::DeadlineExceeded))=>return unknown(intent,ActionUnknownReason::Timeout),
                Err(_)=>return unknown(intent,ActionUnknownReason::ResponseLost),
            };
            if result.validate_for(intent).is_err(){return unknown(intent,ActionUnknownReason::InvalidReceipt);}
            match &result {
                CalendarEffectOutcome::Committed{receipt} if matches!(&receipt.evidence,CalendarReceiptEvidence::NativeAcknowledgement{host_epoch,..} if host_epoch==&self.host_epoch)=>result,
                CalendarEffectOutcome::NotApplied{proof} if proof.host_epoch==self.host_epoch && proof.invocation_id==self.preparation_id=>result,
                CalendarEffectOutcome::Unknown{..}=>result,
                _=>unknown(intent,ActionUnknownReason::InvalidReceipt),
            }
        })
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeActionEnvelope<T>{data:T}

#[derive(serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields)]
struct NativeActionDestinations {
    schema_version:u32,person_id:PersonId,device_id:String,executor_generation:u64,
    source:ActionSourceFence,resources:Vec<CalendarDestinationObservation>,
}

enum NativeActionTransportError {NotInvoked(floe_actions::ActionNotAppliedReason),Unknown(AgentFailure)}
impl From<AgentFailure> for NativeActionTransportError{fn from(value:AgentFailure)->Self{Self::Unknown(value)}}

async fn action_native<T:DeserializeOwned+serde::Serialize+Send+'static>(request:Value,scope:&ExecutionScope,readback:bool)->Result<T,NativeActionTransportError>{
    let input=serde_json::to_string(&request).map_err(|_|NativeActionTransportError::NotInvoked(floe_actions::ActionNotAppliedReason::ProviderRejected))?;
    if input.len()>floe_actions::MAX_ACTION_BYTES{return Err(NativeActionTransportError::NotInvoked(floe_actions::ActionNotAppliedReason::ProviderRejected));}
    if scope.cancellation().is_cancelled(){return Err(AgentFailure::Cancelled.into());}
    if Instant::now()>=scope.deadline(){return Err(AgentFailure::DeadlineExceeded.into());}
    let task=tokio::task::spawn_blocking(move||{
        let bridge=if readback{&EVENT_KIT_RECEIPTS}else{&EVENT_KIT};
        // These three driver errors are returned strictly before invoke().
        // NoResponse/ResponseTooLarge occur after it and remain uncertain.
        let bytes=bridge.call(&input,Some(floe_actions::MAX_ACTION_BYTES)).map_err(|failure|match failure{
            floe_native::NativeCallError::Busy|floe_native::NativeCallError::Unavailable=>NativeActionTransportError::NotInvoked(floe_actions::ActionNotAppliedReason::ProviderUnavailable),
            floe_native::NativeCallError::InvalidRequest=>NativeActionTransportError::NotInvoked(floe_actions::ActionNotAppliedReason::ProviderRejected),
            floe_native::NativeCallError::NoResponse|floe_native::NativeCallError::ResponseTooLarge=>NativeActionTransportError::Unknown(AgentFailure::CapabilityUnavailable),
        })?;
        crate::gateway::json::strict_json_bytes(&bytes,floe_actions::MAX_ACTION_BYTES)?;
        let envelope:NativeActionEnvelope<T>=serde_json::from_slice(&bytes).map_err(|_|AgentFailure::CapabilityUnavailable)?;
        let raw:Value=serde_json::from_slice(&bytes).map_err(|_|AgentFailure::CapabilityUnavailable)?;
        let normalized=serde_json::to_value(&envelope.data).map_err(|_|AgentFailure::CapabilityUnavailable)?;
        if !same_action_shape(&raw["data"],&normalized){return Err(AgentFailure::CapabilityUnavailable.into());}
        Ok(envelope.data)
    });
    tokio::select!{
        biased;
        _=scope.cancellation().cancelled()=>Err(AgentFailure::Cancelled.into()),
        _=tokio::time::sleep_until(scope.deadline())=>Err(AgentFailure::DeadlineExceeded.into()),
        result=task=>result.map_err(|_|NativeActionTransportError::Unknown(AgentFailure::Interrupted))?,
    }
}

// Shared Day values need not make the native codec permissive. Check nested key
// sets after typed decoding while allowing equivalent RFC3339 spellings.
fn same_action_shape(raw:&Value,typed:&Value)->bool{
    match (raw,typed){
        (Value::Object(raw),Value::Object(typed))=>raw.len()==typed.len() && typed.iter().all(|(key,value)|raw.get(key).is_some_and(|raw|same_action_shape(raw,value))),
        (Value::Array(raw),Value::Array(typed))=>raw.len()==typed.len() && raw.iter().zip(typed).all(|(raw,typed)|same_action_shape(raw,typed)),
        (Value::String(_),Value::String(_))|(Value::Number(_),Value::Number(_))|(Value::Bool(_),Value::Bool(_))|(Value::Null,Value::Null)=>true,
        _=>false,
    }
}

fn call_read<T: DeserializeOwned>(request: Value) -> Result<T, ReadCallFailure> {
    let value = invoke_with_limit(request, Some(65_536)).map_err(|failure| match failure {
        InvokeFailure::Action(action) => ReadCallFailure::Action(action),
        InvokeFailure::ResponseTooLarge => ReadCallFailure::BudgetExceeded,
    })?;
    if let Some(reason) = value.get("error") {
        if reason.as_str() == Some("budget_exceeded") {
            return Err(ReadCallFailure::BudgetExceeded);
        }
        return Err(ReadCallFailure::Action(
            serde_json::from_value(reason.clone()).unwrap_or(NativeReadFailure::ProviderUnavailable),
        ));
    }
    serde_json::from_value(value["data"].clone())
        .map_err(|_| ReadCallFailure::Action(NativeReadFailure::UncertainResult))
}

enum InvokeFailure {
    Action(NativeReadFailure),
    ResponseTooLarge,
}

static EVENT_KIT: floe_native::GatedStringCall =
    floe_native::GatedStringCall::new(floe_native::NativeLibrary {
        relative_path: "Frameworks/libfloe_eventkit.dylib",
        invoke_symbol: c"floe_eventkit_action",
        release_symbol: c"floe_eventkit_free",
        bundle_parents: floe_native::MACOS_BUNDLE_ROOT,
    });

// This independent readback gate reaches only the native outcome cache. It never
// enters EventKit or queues behind a still-running write; the Swift cache has its
// own lock and performs exact immutable admission comparisons.
static EVENT_KIT_RECEIPTS:floe_native::GatedStringCall=floe_native::GatedStringCall::new(floe_native::NativeLibrary{
    relative_path:"Frameworks/libfloe_eventkit.dylib",invoke_symbol:c"floe_eventkit_action",release_symbol:c"floe_eventkit_free",bundle_parents:floe_native::MACOS_BUNDLE_ROOT,
});

fn invoke_with_limit(
    request: Value,
    max_response_bytes: Option<usize>,
) -> Result<Value, InvokeFailure> {
    let output = EVENT_KIT
        .call(&request.to_string(), max_response_bytes)
        .map_err(|error| match error {
            floe_native::NativeCallError::ResponseTooLarge => InvokeFailure::ResponseTooLarge,
            _ => InvokeFailure::Action(NativeReadFailure::ProviderUnavailable),
        })?;
    serde_json::from_slice(&output)
        .map_err(|_| InvokeFailure::Action(NativeReadFailure::UncertainResult))
}
