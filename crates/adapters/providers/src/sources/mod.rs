//! Source transports.

pub mod native_acquisition;
pub mod native_calendar;
pub mod personal_native;
pub mod server;

pub use native_acquisition::LocalAcquisitionBrokers;
pub use native_calendar::{NativeCalendar, NativeCalendarReadAccess};
pub use personal_native::NativePersonalDriver;
pub use server::{AuthorizedSourceClient, AuthorizedViewRead, ServerSourceClient};
