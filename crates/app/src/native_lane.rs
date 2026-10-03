//! Independent mechanical callback lifetime. It owns no product service and no
//! core pointer; its verified caller and admission token come from HostRequest.
use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use uuid::Uuid;
use crate::{AgentFailure, AppComposition, CallerContext, HostError, HostRequest, LocalContextHost,
    NativeHostCommand, NativeHostCommands, NativeHostKind, NativeHostOutcome, NativeHostQueries,
    NativeHostQuery, NativeHostRegistrationRef};
use crate::host::{CloseAdmission, HostAdmission};

#[derive(Debug)]
pub enum NativeHostLaneError { Host(HostError), Native(AgentFailure) }
struct Registrations { closed: bool, values: BTreeMap<NativeHostKind, NativeHostRegistrationRef> }
struct LaneState { context: Arc<LocalContextHost>, caller: CallerContext, registrations: Mutex<Registrations> }
impl CloseAdmission for LaneState {
    fn close_admission(&self) {
        let mut registrations = self.registrations.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if registrations.closed { return; }
        registrations.closed = true;
        for (kind, registration) in std::mem::take(&mut registrations.values) {
            // A replaced registration belongs to another lane. Exact comparison
            // in dispose makes stale release harmless to that replacement.
            let _ = self.context.dispose(&self.caller, kind, &registration);
        }
    }
}
pub struct NativeHostLane { admission: Arc<HostAdmission>, state: Arc<LaneState> }
impl HostRequest<'_, AppComposition> {
    pub fn acquire_native_host_lane(&self) -> Result<NativeHostLane, HostError> {
        let admission = self.admission();
        let state = Arc::new(LaneState { context: self.services().local_context.clone(), caller: self.caller().clone(),
            registrations: Mutex::new(Registrations { closed: false, values: BTreeMap::new() }) });
        let hook: Arc<dyn CloseAdmission> = state.clone();
        admission.register_close_hook(Arc::downgrade(&hook))?;
        Ok(NativeHostLane { admission, state })
    }
}
impl NativeHostLane {
    pub fn command(&self, request_id: Uuid, command: NativeHostCommand) -> Result<NativeHostOutcome, NativeHostLaneError> {
        let _guard = self.admission.enter(request_id).map_err(NativeHostLaneError::Host)?;
        let mut registrations = self.state.registrations.lock().map_err(|_| NativeHostLaneError::Host(HostError::Shutdown))?;
        if registrations.closed { return Err(NativeHostLaneError::Host(HostError::Closing)); }
        let (kind, registration, disposing) = match &command {
            NativeHostCommand::Register { kind } => (*kind, None, false),
            NativeHostCommand::Dispose { kind, registration } => (*kind, Some(registration), true),
            NativeHostCommand::CompleteCalendar { registration, .. } | NativeHostCommand::FailCalendar { registration, .. } => (NativeHostKind::Calendar, Some(registration), false),
            NativeHostCommand::CompleteAttention { registration, .. } | NativeHostCommand::FailAttention { registration, .. } => (NativeHostKind::Attention, Some(registration), false),
            NativeHostCommand::CompletePersonal { registration, .. } | NativeHostCommand::FailPersonal { registration, .. } => (NativeHostKind::Personal, Some(registration), false),
        };
        if registration.is_some_and(|value| registrations.values.get(&kind) != Some(value)) {
            return Err(NativeHostLaneError::Native(AgentFailure::PolicyDenied));
        }
        let outcome = self.state.context.apply_native_host(&self.state.caller, command).map_err(NativeHostLaneError::Native)?;
        if let NativeHostOutcome::Registered(reference) = &outcome { registrations.values.insert(kind, reference.clone()); }
        if disposing { registrations.values.remove(&kind); }
        Ok(outcome)
    }
    pub fn query(&self, request_id: Uuid, query: NativeHostQuery) -> Result<NativeHostOutcome, NativeHostLaneError> {
        let _guard = self.admission.enter(request_id).map_err(NativeHostLaneError::Host)?;
        let registrations = self.state.registrations.lock().map_err(|_| NativeHostLaneError::Host(HostError::Shutdown))?;
        if registrations.closed { return Err(NativeHostLaneError::Host(HostError::Closing)); }
        let NativeHostQuery::Poll { kind, registration } = &query;
        if registrations.values.get(kind) != Some(registration) { return Err(NativeHostLaneError::Native(AgentFailure::PolicyDenied)); }
        self.state.context.query_native_host(&self.state.caller, query).map_err(NativeHostLaneError::Native)
    }
    pub fn close(&self) { self.state.close_admission(); }
}
impl Drop for NativeHostLane { fn drop(&mut self) { self.close(); } }
