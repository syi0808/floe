//! Source transports.

pub mod native_acquisition;
pub mod native_calendar;
pub mod calendar_product;
pub mod expert_transport;
pub mod source_metadata;
pub mod personal_native;
pub mod server;

pub use native_acquisition::LocalAcquisitionBrokers;
pub use native_calendar::{NativeCalendarExecutor, NativeCalendarReadAccess};
pub use calendar_product::CalendarProductAdapter;
pub use expert_transport::ExpertSourceAdapter;
pub use source_metadata::NativeSourceMetadataAdapter;
pub use personal_native::NativePersonalDriver;
pub use server::{AuthorizedSourceClient, AuthorizedViewRead, ServerSourceClient};
