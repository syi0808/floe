//! Source transports.

pub mod calendar_product;
pub mod expert_transport;
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
mod fixture_calendar;
pub mod native_acquisition;
pub mod native_calendar;
pub mod personal_native;
pub mod server;
pub mod source_metadata;

pub use calendar_product::CalendarProductAdapter;
pub use expert_transport::ExpertSourceAdapter;
pub use native_acquisition::LocalAcquisitionBrokers;
pub use native_calendar::{NativeCalendarExecutor, NativeCalendarReadAccess};
pub use personal_native::NativePersonalDriver;
pub use server::{AuthorizedSourceClient, AuthorizedViewRead, ServerSourceClient};
pub use source_metadata::NativeSourceMetadataAdapter;
