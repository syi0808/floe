//! The bundled host's acquisition queue.
//!
//! One host process at a time answers acquisition requests for one Person. This
//! broker holds the pending requests, the waiters that are blocked on them, the
//! deadline each one must be answered inside, and the single in-flight slot the
//! host polls. It knows nothing about what is being acquired, what the answer
//! means, or who is allowed to ask.

mod attention;
mod calendar;
mod personal;

pub use attention::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    AttentionBroker, attention_failure,
};
pub use calendar::{
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult, CalendarBroker,
    CalendarSourceFailure, calendar_failure,
};
pub use personal::{
    PersonalAcquisitionRequest, PersonalAcquisitionResult, PersonalBroker, PersonalDomain,
    personal_failure,
};

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::Duration,
};

use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use tokio::time::Instant;
use uuid::Uuid;

/// The most requests one host may have outstanding.
pub const MAX_ACQUISITION_PENDING: usize = 16;
/// The furthest ahead a request's deadline may be.
pub const MAX_ACQUISITION_DEADLINE_MS: i64 = 30_000;

/// What the host's answer is worth, once the request it names is found.
pub enum CompletionOutcome {
    Accept,
    /// Clear the request, its deadline and the in-flight slot, then fail the
    /// waiter.
    Reject(AgentFailure),
    /// Clear the request and the in-flight slot but keep the recorded deadline.
    RejectKeepingDeadline(AgentFailure),
    /// Refuse the answer without disturbing the request it claims to answer.
    Refuse(AgentFailure),
}

/// One kind of acquisition: what a request is, what an answer is, and whether a
/// given answer answers a given request.
pub trait AcquisitionExchange {
    type Request: Clone + Send + 'static;
    type Response: Send + 'static;

    fn request_id(request: &Self::Request) -> Uuid;
    fn request_host_epoch(request: &Self::Request) -> &str;
    fn request_person(request: &Self::Request) -> PersonId;
    fn request_deadline_unix_ms(request: &Self::Request) -> i64;

    fn response_id(response: &Self::Response) -> Uuid;
    fn response_host_epoch(response: &Self::Response) -> &str;

    /// Whether the host's answer is an answer to this request, and what to do
    /// when it is not.
    fn admit(request: &Self::Request, response: &Self::Response) -> CompletionOutcome;

    /// Whether a request with no recorded deadline counts as already expired.
    const MISSING_DEADLINE_EXPIRES: bool;
    /// Whether the answer's own host epoch must equal the polling host's.
    const RESPONSE_CARRIES_HOST_EPOCH: bool = false;
    /// Whether the waiter re-checks cancellation and the deadline after it is
    /// handed a result.
    const RECHECK_AFTER_RECEIVE: bool = false;
}

struct BrokerState<Exchange: AcquisitionExchange> {
    host_epoch: Option<String>,
    host_person: Option<PersonId>,
    pending: VecDeque<Uuid>,
    requests: HashMap<Uuid, Exchange::Request>,
    waiters: HashMap<Uuid, tokio::sync::oneshot::Sender<Result<Exchange::Response, AgentFailure>>>,
    deadlines: HashMap<Uuid, Instant>,
    in_flight: Option<Uuid>,
}

impl<Exchange: AcquisitionExchange> Default for BrokerState<Exchange> {
    fn default() -> Self {
        Self {
            host_epoch: None,
            host_person: None,
            pending: VecDeque::new(),
            requests: HashMap::new(),
            waiters: HashMap::new(),
            deadlines: HashMap::new(),
            in_flight: None,
        }
    }
}

/// Whether registering a host replaced a live one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRegistration {
    /// The same host epoch was already registered; nothing moved.
    Unchanged,
    /// A new host took over; every outstanding request was interrupted.
    Replaced,
}

pub struct AcquisitionBroker<Exchange: AcquisitionExchange> {
    state: Mutex<BrokerState<Exchange>>,
}

impl<Exchange: AcquisitionExchange> Default for AcquisitionBroker<Exchange> {
    fn default() -> Self {
        Self {
            state: Mutex::new(BrokerState::default()),
        }
    }
}

/// Clears the request it names when the caller's scope ends, and nothing else.
struct RequestScope<'broker, Exchange: AcquisitionExchange> {
    broker: &'broker AcquisitionBroker<Exchange>,
    request_id: Uuid,
}

impl<Exchange: AcquisitionExchange> Drop for RequestScope<'_, Exchange> {
    fn drop(&mut self) {
        self.broker.cancel(self.request_id);
    }
}

impl<Exchange: AcquisitionExchange> AcquisitionBroker<Exchange> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The live host epoch for this Person, if they own the host.
    pub fn host_epoch(&self, person_id: PersonId) -> Result<String, AgentFailure> {
        let state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.host_person != Some(person_id) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        state
            .host_epoch
            .clone()
            .ok_or(AgentFailure::CapabilityUnavailable)
    }

    /// Take over the host slot. A new epoch interrupts every waiter the old one
    /// owed an answer to; the Person may never change under a live host.
    pub fn register_host(
        &self,
        person_id: PersonId,
        host_epoch: String,
    ) -> Result<HostRegistration, AgentFailure> {
        let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.host_person.is_some_and(|owner| owner != person_id) {
            return Err(AgentFailure::CapabilityDenied);
        }
        if state.host_epoch.as_deref() == Some(host_epoch.as_str()) {
            if state.host_person != Some(person_id) {
                return Err(AgentFailure::CapabilityDenied);
            }
            return Ok(HostRegistration::Unchanged);
        }
        Self::interrupt_locked(&mut state);
        state.host_epoch = Some(host_epoch);
        state.host_person = Some(person_id);
        Ok(HostRegistration::Replaced)
    }

    pub fn dispose_host(&self, person_id: PersonId, host_epoch: &str) -> Result<(), AgentFailure> {
        let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.host_epoch.as_deref() != Some(host_epoch) || state.host_person != Some(person_id) {
            return Err(AgentFailure::StaleContext);
        }
        Self::interrupt_locked(&mut state);
        state.host_epoch = None;
        state.host_person = None;
        Ok(())
    }

    fn interrupt_locked(state: &mut BrokerState<Exchange>) {
        for (_, waiter) in state.waiters.drain() {
            let _ = waiter.send(Err(AgentFailure::Interrupted));
        }
        state.pending.clear();
        state.requests.clear();
        state.deadlines.clear();
        state.in_flight = None;
    }

    /// Hand the host the next request it may answer, if it is not already
    /// holding one. Requests whose deadline has passed are failed, not shown.
    pub fn poll(
        &self,
        person_id: PersonId,
        host_epoch: &str,
    ) -> Result<Vec<Exchange::Request>, AgentFailure> {
        let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.host_epoch.as_deref() != Some(host_epoch) || state.host_person != Some(person_id) {
            return Err(AgentFailure::StaleContext);
        }
        if state.in_flight.is_some() {
            return Ok(Vec::new());
        }
        while let Some(request_id) = state.pending.pop_front() {
            if let Some(request) = state.requests.get(&request_id).cloned() {
                if state
                    .deadlines
                    .get(&request_id)
                    .is_none_or(|deadline| *deadline <= Instant::now())
                {
                    state.requests.remove(&request_id);
                    state.deadlines.remove(&request_id);
                    if let Some(waiter) = state.waiters.remove(&request_id) {
                        let _ = waiter.send(Err(AgentFailure::DeadlineExceeded));
                    }
                    continue;
                }
                state.in_flight = Some(request_id);
                return Ok(vec![request]);
            }
        }
        Ok(Vec::new())
    }

    /// Deliver the host's answer to the waiter that asked for it.
    pub fn complete(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        response: Exchange::Response,
    ) -> Result<(), AgentFailure> {
        let request_id = Exchange::response_id(&response);
        let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || state.in_flight != Some(request_id)
            || (Exchange::RESPONSE_CARRIES_HOST_EPOCH
                && Exchange::response_host_epoch(&response) != host_epoch)
        {
            return Err(AgentFailure::StaleContext);
        }
        let expired = match state.deadlines.get(&request_id) {
            Some(deadline) => *deadline <= Instant::now(),
            None => Exchange::MISSING_DEADLINE_EXPIRES,
        };
        if expired {
            Self::reject_locked(&mut state, request_id, AgentFailure::DeadlineExceeded, true);
            return Err(AgentFailure::DeadlineExceeded);
        }
        let Some(request) = state.requests.get(&request_id) else {
            return Err(AgentFailure::StaleContext);
        };
        match Exchange::admit(request, &response) {
            CompletionOutcome::Accept => {}
            CompletionOutcome::Refuse(failure) => return Err(failure),
            CompletionOutcome::Reject(failure) => {
                Self::reject_locked(&mut state, request_id, failure, true);
                return Err(failure);
            }
            CompletionOutcome::RejectKeepingDeadline(failure) => {
                Self::reject_locked(&mut state, request_id, failure, false);
                return Err(failure);
            }
        }
        let waiter = state.waiters.remove(&request_id);
        state.requests.remove(&request_id);
        state.deadlines.remove(&request_id);
        state.in_flight = None;
        if let Some(waiter) = waiter {
            let _ = waiter.send(Ok(response));
        }
        Ok(())
    }

    /// The host reports it could not answer the request it is holding.
    pub fn fail(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        request_id: Uuid,
        failure: AgentFailure,
    ) -> Result<(), AgentFailure> {
        let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.host_epoch.as_deref() != Some(host_epoch)
            || state.host_person != Some(person_id)
            || state.in_flight != Some(request_id)
        {
            return Err(AgentFailure::StaleContext);
        }
        Self::reject_locked(&mut state, request_id, failure, true);
        Ok(())
    }

    /// Release the in-flight request without reporting an error to the host.
    ///
    /// A stale host, or one that is not holding this request, changes nothing.
    pub fn reject(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        request_id: Uuid,
        failure: AgentFailure,
    ) {
        if let Ok(mut state) = self.state.lock() {
            if state.host_epoch.as_deref() != Some(host_epoch)
                || state.host_person != Some(person_id)
                || state.in_flight != Some(request_id)
            {
                return;
            }
            Self::reject_locked(&mut state, request_id, failure, true);
        }
    }

    fn reject_locked(
        state: &mut BrokerState<Exchange>,
        request_id: Uuid,
        failure: AgentFailure,
        clear_deadline: bool,
    ) {
        state.requests.remove(&request_id);
        if clear_deadline {
            state.deadlines.remove(&request_id);
        }
        state.in_flight = None;
        if let Some(waiter) = state.waiters.remove(&request_id) {
            let _ = waiter.send(Err(failure));
        }
    }

    /// Drop one request. A scope that ends cleans up only what it asked for.
    pub fn cancel(&self, request_id: Uuid) {
        if let Ok(mut state) = self.state.lock() {
            state.pending.retain(|value| *value != request_id);
            state.requests.remove(&request_id);
            if state.in_flight == Some(request_id) {
                state.in_flight = None;
            }
            state.deadlines.remove(&request_id);
            if let Some(waiter) = state.waiters.remove(&request_id) {
                let _ = waiter.send(Err(AgentFailure::Cancelled));
            }
        }
    }

    /// Ask the live host for one acquisition and wait for its answer.
    pub async fn submit(
        &self,
        request: Exchange::Request,
        wall_now_unix_ms: i64,
        cancellation: Cancellation,
    ) -> Result<Exchange::Response, AgentFailure> {
        let deadline_unix_ms = Exchange::request_deadline_unix_ms(&request);
        if deadline_unix_ms <= wall_now_unix_ms {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let duration = Duration::from_millis(
            u64::try_from(deadline_unix_ms - wall_now_unix_ms)
                .map_err(|_| AgentFailure::DeadlineExceeded)?,
        );
        if duration > Duration::from_millis(MAX_ACQUISITION_DEADLINE_MS as u64) {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let monotonic_deadline = Instant::now() + duration;
        let request_id = Exchange::request_id(&request);
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let mut state = self.state.lock().map_err(|_| AgentFailure::Interrupted)?;
            if state.host_epoch.as_deref() != Some(Exchange::request_host_epoch(&request)) {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            if state.host_person != Some(Exchange::request_person(&request)) {
                return Err(AgentFailure::CapabilityDenied);
            }
            if state.requests.len() >= MAX_ACQUISITION_PENDING {
                return Err(AgentFailure::BudgetExceeded);
            }
            if state.requests.contains_key(&request_id) {
                return Err(AgentFailure::InvalidInput);
            }
            state.pending.push_back(request_id);
            state.requests.insert(request_id, request);
            state.waiters.insert(request_id, sender);
            state.deadlines.insert(request_id, monotonic_deadline);
        }
        let _scope = RequestScope {
            broker: self,
            request_id,
        };
        let received = tokio::select! {
            result = receiver => result.map_err(|_| AgentFailure::Interrupted)?,
            _ = cancellation.cancelled() => {
                self.cancel(request_id);
                return Err(AgentFailure::Cancelled);
            }
            _ = tokio::time::sleep_until(monotonic_deadline) => {
                self.cancel(request_id);
                return Err(AgentFailure::DeadlineExceeded);
            }
        };
        if Exchange::RECHECK_AFTER_RECEIVE {
            if cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            if Instant::now() >= monotonic_deadline {
                return Err(AgentFailure::DeadlineExceeded);
            }
        }
        received
    }
}
