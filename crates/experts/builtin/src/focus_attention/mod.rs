mod dispatch;
mod expert;

pub use dispatch::{CONSUMER, dispatch};
pub use expert::*;
pub const RESULT_MEDIA_TYPE: &str = "application/vnd.floe.expert.focus-attention+json;version=1";
