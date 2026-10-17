//! Live-session lifecycle policy, independent of OS media and provider protocol.
//!
//! This owner state emits intent; it neither grants microphone permission nor
//! starts transport. Native/control adapters must enforce the bound lease and
//! generation before effects. Durable composition is a separate pending slice.
mod session;
pub use session::{
    MediaCommand, MediaCommandKind, MediaState, RemoteState, VoiceBinding, VoiceSession,
};
