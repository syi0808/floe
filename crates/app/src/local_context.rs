//! Mechanical host registration and acquisition lifetime wiring. Product input
//! cannot publish a trusted view; only an outstanding native acquisition may
//! complete under its exact caller, registration and native operation identity.
use std::{collections::BTreeMap, sync::{Mutex, Arc}};
use floe_agent_contract::AgentFailure;
use floe_context::ObservationRegistry;
use floe_kernel::PersonId;
use floe_provider_adapters::sources::native_acquisition::{AttentionBroker, CalendarBroker,
    CalendarAcquisitionRequest, AttentionAcquisitionRequest, PersonalAcquisitionRequest,
    LocalAcquisitionBrokers, PersonalBroker};
use uuid::Uuid;
use crate::CallerContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum NativeHostKind { Calendar, Attention, Personal }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeHostRegistrationRef {
    pub registration_id: Uuid,
    pub host_epoch: String,
    pub runtime_epoch: u64,
}
struct RegisteredNativeHost { reference: NativeHostRegistrationRef, person_id: PersonId, device_id: String }

pub enum NativeHostOutcome {
    Registered(NativeHostRegistrationRef),
    CalendarAcquisitions(Vec<CalendarAcquisitionRequest>),
    AttentionAcquisitions(Vec<AttentionAcquisitionRequest>),
    PersonalAcquisitions(Vec<PersonalAcquisitionRequest>),
    Acknowledged,
}
pub struct LocalContextHost {
    brokers: LocalAcquisitionBrokers,
    observations: Arc<ObservationRegistry>,
    registrations: Mutex<BTreeMap<NativeHostKind, RegisteredNativeHost>>,
}
impl Default for LocalContextHost {
    fn default() -> Self { Self { brokers: LocalAcquisitionBrokers::new(), observations: Arc::new(ObservationRegistry::new()), registrations: Mutex::new(BTreeMap::new()) } }
}
impl LocalContextHost {
    pub fn calendar(&self) -> &CalendarBroker { self.brokers.calendar() }
    pub fn attention(&self) -> &AttentionBroker { self.brokers.attention() }
    pub fn personal(&self) -> &PersonalBroker { self.brokers.personal() }
    pub(crate) fn attention_handle(&self) -> Arc<AttentionBroker> { self.brokers.attention_handle() }
    pub(crate) fn personal_handle(&self) -> Arc<PersonalBroker> { self.brokers.personal_handle() }
    pub(crate) fn observations_handle(&self) -> Arc<ObservationRegistry> { self.observations.clone() }
    pub fn observations(&self) -> &ObservationRegistry { &self.observations }
    pub fn process_incarnation(&self) -> Uuid { self.observations.process_incarnation() }

    pub(crate) fn register(&self, caller: &CallerContext, kind: NativeHostKind) -> Result<NativeHostRegistrationRef, AgentFailure> {
        let person_id = PersonId(caller.person_id());
        let reference = NativeHostRegistrationRef { registration_id: Uuid::new_v4(), host_epoch: Uuid::new_v4().to_string(), runtime_epoch: caller.runtime_epoch() };
        let mut registrations = self.registrations.lock().map_err(|_| AgentFailure::Interrupted)?;
        if registrations.values().any(|registration| registration.person_id != person_id || registration.device_id != caller.device_id()) { return Err(AgentFailure::PolicyDenied); }
        match kind {
            NativeHostKind::Calendar => { self.calendar().register_host(person_id, reference.host_epoch.clone())?; }
            NativeHostKind::Attention => { self.observations.invalidate_trusted_attention(person_id)?; self.attention().register_host(person_id, reference.host_epoch.clone())?; }
            NativeHostKind::Personal => { self.personal().register_host(person_id, reference.host_epoch.clone())?; }
        }
        registrations.insert(kind, RegisteredNativeHost { reference: reference.clone(), person_id, device_id: caller.device_id().to_owned() });
        Ok(reference)
    }
    pub(crate) fn validate_registration(&self, caller: &CallerContext, kind: NativeHostKind, reference: &NativeHostRegistrationRef) -> Result<(), AgentFailure> {
        let registrations = self.registrations.lock().map_err(|_| AgentFailure::Interrupted)?;
        let registered = registrations.get(&kind).ok_or(AgentFailure::StaleContext)?;
        if reference.registration_id.is_nil() || reference.runtime_epoch != caller.runtime_epoch()
            || registered.reference != *reference || registered.person_id.0 != caller.person_id()
            || registered.device_id != caller.device_id() { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }
    pub(crate) fn dispose(&self, caller: &CallerContext, kind: NativeHostKind, reference: &NativeHostRegistrationRef) -> Result<(), AgentFailure> {
        let mut registrations = self.registrations.lock().map_err(|_| AgentFailure::Interrupted)?;
        let registered = registrations.get(&kind).ok_or(AgentFailure::StaleContext)?;
        if registered.reference != *reference || registered.person_id.0 != caller.person_id()
            || registered.device_id != caller.device_id() || reference.runtime_epoch != caller.runtime_epoch() { return Err(AgentFailure::PolicyDenied); }
        let person = PersonId(caller.person_id());
        match kind {
            NativeHostKind::Calendar => self.calendar().dispose_host(person, &reference.host_epoch)?,
            NativeHostKind::Attention => { self.attention().dispose_host(person, &reference.host_epoch)?; self.observations.invalidate_trusted_attention(person)?; }
            NativeHostKind::Personal => self.personal().dispose_host(person, &reference.host_epoch)?,
        }
        registrations.remove(&kind);
        Ok(())
    }
}
