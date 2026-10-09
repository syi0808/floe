//! Actions owns one encrypted record for every manual or Expert Calendar effect,
//! its exact authorization, durable dispatch, causal evidence and Day collection.
//! External provider I/O and encrypted transactions live behind owner ports.
mod application;
mod domain;
mod ports;

pub use application::{CalendarOperationsDependencies, CalendarOperationsService};
pub use domain::*;
pub use ports::*;
