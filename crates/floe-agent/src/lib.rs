mod a2a;
mod calendar_context;
mod capability_execution;
mod communication_context;
mod connected_context;
mod context_routing;
mod contract;
mod experts;
mod learner;
mod learning;
mod model_attempt;
mod model_journal;
mod model_usage;
mod native_context;
mod personal_context;
mod playbook;
mod policy;
mod portfolio_context;
mod prompts;
mod registry;
mod runtime;

pub use floe_agent_contract::{
    AgentFailure, DataClass, ModelPlacement, SessionProtection, TransferConsent,
};

pub use a2a::{
    A2A_PROTOCOL_VERSION, A2AArtifact, A2AHost, A2AMessage, A2AMessageRole, A2APart, A2ARouter,
    A2ASendMessageRequest, A2ATask, A2ATaskRequest, A2ATaskState, AgentCard,
    EXPERT_RESULT_MEDIA_TYPE, InProcessA2ATransport, InProcessAgent, NoA2AHost,
};
pub use calendar_context::{
    CALENDAR_CONTEXT_VIEW_ID, CalendarContextItem, CalendarContextView, MAX_CALENDAR_CONTEXT_BYTES,
    MAX_CALENDAR_CONTEXT_ITEMS, calendar_context_evidence, validate_calendar_context_view,
};
pub use communication_context::{
    COMMUNICATION_VIEW_ID, CommunicationItem, CommunicationView, MAX_COMMUNICATION_BYTES,
    MAX_COMMUNICATION_FRESHNESS_MS, MAX_COMMUNICATION_ITEMS, communication_context_evidence,
    validate_communication_view,
};
pub use connected_context::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConformanceCode, ConformanceViolation,
    ConnectionState, ConnectorCapabilityDescriptor, ConnectorConnectionSnapshot,
    ConnectorDescriptor, ConnectorSnapshot, DeviceBinding, ExecutionLocation, RetentionClass,
    SituationConformanceReport, SituationDescriptor, SituationTrigger, SourceFailure,
    SourceFailureKind, SourceIssue, ViewDescriptor, ViewSnapshot, evaluate_situation,
    validate_connector_snapshot,
};
pub use context_routing::{
    ContextRouteResult, ContextRoutingRuntime, ContextTransferClass, DeviceClass, DevicePresence,
    DeviceScope, LogicalViewRoute, LogicalViewRoutingPolicy, RouteDisagreement, RoutedAvailability,
    RoutedView, RuntimeDeviceState, RuntimeSourcePolicy, route_logical_views,
    route_logical_views_with_runtime,
};
pub use contract::{
    AGENT_VERSION, AgentBudget, AgentCardManifestEntry, AgentCommand, AgentContinuation,
    AgentEvent, AgentEventKind, AgentMessage, AgentOutcome, AgentSession, AgentSessionScope,
    AgentUsage, CapabilityDescriptor, CapabilityExecution, CapabilityExecutionState,
    CapabilityHost, CapabilityInvocation, ContextEnvelope, ContextManifest, ContextualData,
    ConversationContext, DelegationExecution, DelegationExecutionState, EvidenceManifestEntry,
    MemoryManifestEntry, ModelReplay, ModelRequest, ModelResponse, ModelRunner, ModelStep,
    PromptManifestEntry, ProviderReplay, RuntimeContext, ScopedInstructions,
    SessionRecoveryPointer, SessionStore,
};
pub use experts::{
    BUILTIN_EXPERT_PACKAGE_VERSION, CONFIRMED_INTERACTION_VIEW_ID, ConfirmedInteraction,
    ConfirmedInteractionView, FocusContextViews, FocusExpertResult, FocusRecommendation,
    PersonalExpertInvocation, RelationshipFollowUp, RelationshipsContextViews,
    RelationshipsExpertResult, ScheduleImpact, WellbeingContextViews, WellbeingExpertResult,
    run_focus_expert_with_views, run_relationships_expert_with_views,
    run_wellbeing_expert_with_views, validate_confirmed_interaction_view,
};
pub use experts::{
    CommitmentEvidenceSource, CommitmentFinding, CommitmentKind, CommitmentsContextViews,
    CommitmentsExpertResult, CommunicationAssessment, CommunicationChannel,
    CommunicationExpertResult, CommunicationResultKind, FindingEpistemicStatus,
    MailExpertInvocation, run_commitments_expert_with_views, run_communication_expert,
};
pub use experts::{
    ExpertBudget, ExpertFocusProposal, ExpertHost, ExpertInput, ExpertInsight, ExpertInvocation,
    ExpertResult, ExpertTimelineView, ExpertViews, MAX_TIMELINE_VIEW_BYTES, MAX_TIMELINE_VIEW_DAYS,
    MAX_TIMELINE_VIEW_ITEMS, TimelineViewItem, TimelineViewRead,
};
pub use experts::{
    LifeLogisticsExpertResult, LogisticsPreparation, LogisticsUrgency, PortfolioExpertInvocation,
    WorkContextExpertResult, WorkInsight, run_life_logistics_expert, run_work_context_expert,
};
pub use learner::{
    LearnerBudget, LearnerJobSettlement, LearnerJobState, LearnerMemoryProposal, LearnerModel,
    LearnerModelRequest, LearnerReviewInput, LearnerReviewJob, LearnerReviewOutput, LearnerRuntime,
    MemoryCandidateSink, StructuredLearnerModel, explicit_learning_signal,
    retryable_learner_failure,
};
pub use learning::{
    EpistemicStatus, KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate,
    KnowledgeCandidateState, KnowledgeDecision, KnowledgeDecisionKind, KnowledgeDecisionResult,
    KnowledgeKind, KnowledgeMutation, KnowledgeOperation, KnowledgePayload, KnowledgeRevision,
    KnowledgeRevisionState, LearningEvidenceRef, LearningObservation, LearningObservationKind,
    PersonalMemoryKind, PersonalMemoryValue, StageMemoryCandidate,
};
pub use model_attempt::generate_with_recovery;
pub use model_journal::{ModelAttemptRecord, ModelAttemptState};
pub use model_usage::{ModelUsage, UsageAttempt, UsageLedger};
pub use native_context::{
    FLOE_NOTE_VIEW_ID, FLOE_TASK_VIEW_ID, MAX_NATIVE_CONTEXT_BYTES, MAX_NATIVE_CONTEXT_ITEMS,
    MAX_NATIVE_CONTEXT_TEXT_BYTES, NativeContextItem, NativeContextView, TaskContextPriority,
    native_context_evidence, validate_native_context_view,
};
pub use personal_context::{
    ATTENTION_VIEW_ID, AttentionState, AttentionView, CapacityState, FEASIBILITY_VIEW_ID,
    FeasibilityItem, FeasibilityView, MAX_PERSONAL_CONTEXT_BYTES, PEOPLE_VIEW_ID, PeopleIdentity,
    PeopleView, PersonalContextProjection, RecoveryState, WELLBEING_VIEW_ID, WeatherImpact,
    WellbeingView, personal_context_evidence, validate_attention_view, validate_feasibility_view,
    validate_people_view, validate_wellbeing_view,
};
pub use playbook::{
    LoadedPlaybook, MAX_LOADED_PLAYBOOK_BYTES, MAX_LOADED_PLAYBOOKS, MAX_PLAYBOOK_DEPTH,
    MAX_VISIBLE_PLAYBOOKS, Playbook, PlaybookBody, PlaybookChild, PlaybookIndexEntry, PlaybookRef,
    PlaybookRegistry, PlaybookSession,
};
pub use policy::{
    AgentContext, ContextEvidence, ContextMemory, InferencePolicyDecision, MAX_CONTEXT_EVIDENCE,
    MAX_CONTEXT_EVIDENCE_BYTES, MAX_CONTEXT_MEMORIES, MAX_CONTEXT_MEMORY_BYTES,
};
pub use portfolio_context::{
    LOGISTICS_VIEW_ID, LogisticsItem, LogisticsItemKind, LogisticsView, MAX_PORTFOLIO_VIEW_BYTES,
    WORK_CONTEXT_VIEW_ID, WorkContextItem, WorkContextView, WorkItemKind,
    logistics_context_evidence, validate_logistics_view, validate_work_context_view,
    work_context_evidence,
};
pub use prompts::{
    PersonaProfile, PromptAssembly, PromptComponent, PromptComponentKind, PromptRole,
    commitments_expert_prompt, communication_expert_prompt, fixture_follow_up_prompt,
    fixture_repeated_call_prompt, fixture_today_prompt, fixture_unavailable_prompt,
    focus_expert_prompt, learner_prompt, life_logistics_expert_prompt, manager_prompt,
    relationships_expert_prompt, schedule_expert_prompt, wellbeing_expert_prompt,
    work_context_expert_prompt,
};
pub use registry::{
    AgentPackage, AgentRegistry, AssignmentOverview, BuiltinContextSource,
    BuiltinExpertAssignmentReceipt, BuiltinExpertKind, BuiltinExpertSetup,
    BuiltinExpertSetupReceipt, BuiltinExpertSetupResult, BuiltinSourceBinding, BuiltinSourceState,
    CalendarAccessChange, CalendarAccessConfiguration, CalendarExpertOverview, CalendarExpertSetup,
    CalendarExpertSetupReceipt, CalendarExpertSetupResult, CalendarViewBinding, ExpertMetadata,
    ExpertPrivateState, ExpertRule, PackageAssignment, PackageImplementation, PackageInstallation,
    PackageKind, PackageRef, RegistryConfiguration, RegistryConfigurationTarget, RegistryOverview,
    RegistrySnapshot,
};
pub use runtime::{AgentRuntime, Cancellation};
