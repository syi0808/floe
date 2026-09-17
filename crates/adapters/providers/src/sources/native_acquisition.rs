//! The device acquisition queues this process owns.
//!
//! The queues themselves are the platform's. This factory is what the
//! composition root constructs and injects, so that the app never names the
//! native layer directly.

pub use floe_native::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionAcquisitionResult,
    AttentionBroker, CalendarAcquisitionMode, CalendarAcquisitionRequest,
    CalendarAcquisitionResult, CalendarBroker, CalendarSourceFailure, HostRegistration,
    MAX_ACQUISITION_DEADLINE_MS, NativeCalendarBatch, NativeCalendarFailure, NativeCalendarRecord,
    NativeEventSchedule, PersonalAcquisitionRequest, PersonalAcquisitionResult, PersonalBroker,
    PersonalDomain, attention_failure, calendar_failure, personal_failure,
};

/// One host per kind, for one process.
#[derive(Default)]
pub struct LocalAcquisitionBrokers {
    calendar: CalendarBroker,
    attention: AttentionBroker,
    personal: PersonalBroker,
}

impl LocalAcquisitionBrokers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calendar(&self) -> &CalendarBroker {
        &self.calendar
    }

    pub fn attention(&self) -> &AttentionBroker {
        &self.attention
    }

    pub fn personal(&self) -> &PersonalBroker {
        &self.personal
    }
}
