//! Platform layer: OS handles, bundled native drivers, key access and the
//! thread constraints they impose.
//!
//! No product judgment lives here. Adapters map these results onto owner ports.

pub mod acquisition;
pub mod calendar_wire;
pub mod dylib;
mod host;
mod keychain;

pub use acquisition::{
    AcquisitionBroker, AcquisitionExchange, AttentionAcquisitionMode, AttentionAcquisitionRequest,
    AttentionAcquisitionResult, AttentionBroker, CalendarAcquisitionMode,
    CalendarAcquisitionRequest, CalendarAcquisitionResult, CalendarBroker, CalendarSourceFailure,
    CompletionOutcome, HostRegistration, MAX_ACQUISITION_DEADLINE_MS, MAX_ACQUISITION_PENDING,
    PersonalAcquisitionRequest, PersonalAcquisitionResult, PersonalBroker, PersonalDomain,
    attention_failure, calendar_failure, personal_failure,
};
pub use calendar_wire::{
    NATIVE_CALENDAR_WIRE_VERSION, NativeCalendarBatch, NativeCalendarFailure, NativeCalendarRecord,
    NativeEventSchedule,
};
pub use dylib::{
    BUNDLE_SIBLING, ByteCall, GatedStringCall, MACOS_BUNDLE_ROOT, NativeCallError, NativeLibrary,
};
pub use host::{NativeIdentityError, NativeLocalIdentity, local_identity_for_database};
pub use keychain::{KeychainError, read_generic_password};
