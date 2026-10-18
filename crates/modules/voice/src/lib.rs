//! Live-session lifecycle policy, independent of OS media and provider protocol.
//!
//! This owner state emits intent; it neither grants microphone permission nor
//! starts transport. Native/control adapters must enforce the bound lease and
//! generation before effects. The service owns custody and Conversation wakeup,
//! not provider/native transport or actual microphone readiness.
mod session;
pub use session::{
    MediaCommand, MediaCommandKind, MediaState, RemoteState, VoiceBinding, VoiceSession,
};

mod journal;
pub use journal::{
    VoiceCallRecord, VoiceJournal, VoiceMutation, VoiceMutationAdmission, VoiceMutationKind,
    VoiceMutationReceipt, VoiceStartAdmission, VoiceStartIntent,
};

mod ingress;
pub use ingress::{
    VoiceEventOutcome, VoiceEventRecord, VoiceIngressAdmission, VoiceIngressJournal,
    VoiceIngressPlan, VoiceInputCorrelation, VoiceInputRejection, VoiceUnroutedReason,
    event_digest, event_kind, input_command_id, input_lookup_digest, input_rejection, plan_ingress,
    validate_input_link, validate_work_link, work_command_id,
};

mod service;
pub use service::{VoiceController, VoiceIngressResult, VoiceService};

mod driver;
pub use driver::VoiceLiveCompletion;

mod custody;
