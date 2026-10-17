mod service;

pub use service::{InferenceAvailability, InferenceService};

mod selection;

mod live;
pub use live::PreparedLiveSelection;
