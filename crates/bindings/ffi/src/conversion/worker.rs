//! The worker's app wire, read and written once.
//!
//! Everything the host sends arrives as a request DTO and leaves here as the
//! app's own worker command; everything a command produced leaves as a result
//! DTO. Which owner runs a command, and what its answer means, is decided past
//! this boundary — what is decided here is only how the two are said on a wire.

use floe_app::{
    AgentFailure, CalendarActionOperation, CalendarActionProposal, CalendarActionState,
    CalendarProposalInspection, CalendarSubjectPreview, CalendarSubjectRequest,
    ContactsAccessChange, ContactsAccessConfiguration, ConversationSessionOperation,
    FeasibilityGrantQuery, FixtureOperation, GrantState, MemoryReviewDecision, MemoryReviewResult,
    PairingIssuer, PersonId, PersonalAccessChange, PersonalAccessConfiguration,
    ProcessingRestriction, RemoteCalendarGrantPreview, RemoteEnrollmentStatus,
    RemoteOwnerPublicKey, RemoteProducerIdentity, VaultState, WorkerAction, WorkerOperation,
    WorkerResult,
};
use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::*;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

fn parse_uuid(value: &str, field: &'static str) -> WireResult<Uuid> {
    Uuid::parse_str(value).map_err(|_| invalid(field, "must be a UUID"))
}

pub(crate) fn remote_calendar_grant_overview(
    grant: &floe_app::DataAccessGrant,
) -> Result<RemoteCalendarGrantOverviewDto, AgentFailure> {
    let resource = grant
        .scope()
        .resources()
        .first()
        .ok_or(AgentFailure::VaultUnavailable)?
        .as_str()
        .to_owned();
    let consumer = grant
        .scope()
        .consumers()
        .iter()
        .find(|candidate| candidate.identifier() == "calendar.expert")
        .ok_or(AgentFailure::VaultUnavailable)?
        .identifier()
        .to_owned();
    let recipient = match grant.scope().processing() {
        ProcessingRestriction::ApprovedRecipient { recipient, .. } => recipient.clone(),
        ProcessingRestriction::LocalOnly => "local_only".into(),
    };
    Ok(RemoteCalendarGrantOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: grant.source().person_id().to_string(),
        grant_id: grant.id(),
        grant_authority: grant.authority(),
        connector_id: grant.source().connector().as_str().to_owned(),
        connection_id: grant.source().connection_id().as_str().to_owned(),
        resource,
        source_authority: grant.source().source_authority(),
        execution_owner: grant.source().execution_owner().as_str().to_owned(),
        state: match grant.state() {
            GrantState::Paused => "paused",
            GrantState::Active => "active",
            GrantState::Revoked => "revoked",
        }
        .into(),
        review_required: grant.review_required(),
        consumer,
        purpose: "everyday_assistance".into(),
        recipient,
    })
}

pub(crate) fn remote_view_grant_overview(
    grant: &floe_app::DataAccessGrant,
    connection_revision: Option<u64>,
) -> Result<RemoteViewGrantOverviewDto, AgentFailure> {
    let resource = grant
        .scope()
        .resources()
        .first()
        .ok_or(AgentFailure::VaultUnavailable)?
        .as_str()
        .to_owned();
    let (view_id, _) = resource
        .split_once(':')
        .ok_or(AgentFailure::VaultUnavailable)?;
    let consumer = grant
        .scope()
        .consumers()
        .first()
        .ok_or(AgentFailure::VaultUnavailable)?
        .identifier()
        .to_owned();
    let recipient = match grant.scope().processing() {
        ProcessingRestriction::ApprovedRecipient { recipient, .. } => recipient.clone(),
        ProcessingRestriction::LocalOnly => "local_only".into(),
    };
    Ok(RemoteViewGrantOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: grant.source().person_id().to_string(),
        grant_id: grant.id(),
        grant_authority: grant.authority(),
        view_id: view_id.into(),
        connector_id: grant.source().connector().as_str().into(),
        connection_id: grant.source().connection_id().as_str().into(),
        connection_revision,
        resource,
        source_authority: grant.source().source_authority(),
        execution_owner: grant.source().execution_owner().as_str().into(),
        state: match grant.state() {
            GrantState::Paused => "paused",
            GrantState::Active => "active",
            GrantState::Revoked => "revoked",
        }
        .into(),
        review_required: grant.review_required(),
        consumer,
        purpose: "everyday_assistance".into(),
        recipient,
    })
}

pub(crate) fn remote_view_grant_preview(
    person_id: PersonId,
    preview: &floe_app::RemoteViewGrantPreview,
) -> RemoteViewGrantPreviewDto {
    RemoteViewGrantPreviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: person_id.to_string(),
        view_id: preview.reference.view_id.clone(),
        connector_id: preview.reference.connector_id.clone(),
        connection_id: preview.reference.connection_id.clone(),
        connection_revision: preview.connection_revision,
        resource: preview.reference.resource.clone(),
        source_authority: preview.reference.source_authority,
        provider_identity: preview.reference.provider_identity.clone(),
        execution_owner: preview.reference.execution_owner.clone(),
        producer: RemoteProducerIdentityDto {
            schema_version: preview.producer.schema_version,
            instance_id: preview.producer.instance_id.clone(),
            execution_owner: preview.producer.execution_owner.clone(),
            audience: preview.producer.audience.clone(),
            key_id: preview.producer.key_id.clone(),
            public_key: preview.producer.public_key.clone(),
            fingerprint: preview.producer.fingerprint.clone(),
        },
        consumer: preview.consumer.clone(),
        purpose: "everyday_assistance".into(),
        recipient: preview.producer.audience.clone(),
    }
}

fn calendar_action(action: floe_app::CalendarAction) -> AgentProposalActionDto {
    AgentProposalActionDto {
        action_id: action.id.to_string(),
        execution_id: action.execution_id.to_string(),
        expires_at: action.expires_at,
        status: match action.state {
            CalendarActionState::Pending => AgentProposalStatusDto::Pending,
            CalendarActionState::Approved => AgentProposalStatusDto::Approved,
            CalendarActionState::Rejected => AgentProposalStatusDto::Rejected,
            CalendarActionState::Executing => AgentProposalStatusDto::Executing,
            CalendarActionState::Blocked { .. } => AgentProposalStatusDto::Blocked,
            CalendarActionState::Unknown { .. } => AgentProposalStatusDto::Unknown,
            CalendarActionState::Succeeded { .. } => AgentProposalStatusDto::Succeeded,
        },
    }
}

fn decode_contract<T: DeserializeOwned>(value: &impl Serialize) -> Result<T, AgentFailure> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|_| AgentFailure::InvalidInput)
}

fn encode_contracts<Input: Serialize, Output: DeserializeOwned>(
    values: Vec<Input>,
) -> Result<Vec<Output>, AgentFailure> {
    values
        .iter()
        .map(decode_contract)
        .collect::<Result<Vec<_>, _>>()
}

fn encode_contract<T: DeserializeOwned>(value: &impl Serialize) -> Result<T, AgentFailure> {
    decode_contract(value)
}

pub(crate) fn failure_envelope(
    failure: &AgentFailure,
    stage: &str,
    request_id: &str,
) -> AgentVaultFailureDto {
    let kind = serde_json::to_value(failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let classification = classify_failure(failure, stage);
    let recovery = recovery_action(failure, stage);
    AgentVaultFailureDto {
        schema_version: PROTOCOL_VERSION,
        domain: classification.domain,
        category: classification.category,
        reason_code: classification.reason_code,
        kind: kind.clone(),
        stage: stage.into(),
        safe_actions: classification.safe_actions,
        affected_refs: vec![],
        incident_id: request_id.into(),
        retry_policy: classification.retry_policy,
        retryable: classification.retryable,
        recovery_action: recovery,
        reload_required: reload_required(recovery),
        seal_session: seal_session(failure, recovery),
        correlation_request_id: request_id.into(),
    }
}

struct FailureClassification {
    domain: AgentFailureDomain,
    category: AgentFailureCategory,
    reason_code: String,
    safe_actions: Vec<AgentFailureSafeAction>,
    retry_policy: AgentRetryPolicy,
    retryable: bool,
}

fn classify_failure(failure: &AgentFailure, stage: &str) -> FailureClassification {
    let reason_code = serde_json::to_value(failure)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let source_stage = matches!(
        stage,
        "calendar_access"
            | "calendar_experts"
            | "calendar_subject_preview"
            | "calendar_action"
            | "personal_access"
            | "contacts_access"
            | "remote_authority_inspect_producer"
            | "remote_authority_review_and_enroll"
            | "remote_authority_enrollment_status"
            | "remote_pairing_prepare"
            | "remote_pairing_confirm"
            | "remote_pairing_status"
            | "remote_pairing_finalize"
            | "remote_calendar_grant_preview"
            | "remote_calendar_grant_review"
            | "remote_calendar_grant_status"
            | "remote_calendar_grant_pause"
            | "remote_view_grant_preview"
            | "remote_view_grant_review"
            | "remote_view_grant_status"
            | "remote_view_grant_pause"
    );
    let (domain, category, reason_code) = match failure {
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => (
            AgentFailureDomain::Vault,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        AgentFailure::PolicyDenied if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            "session_integrity".into(),
        ),
        AgentFailure::PolicyDenied if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Security,
            "data_release_or_policy_block".into(),
        ),
        AgentFailure::PolicyDenied if source_stage => (
            AgentFailureDomain::Source,
            AgentFailureCategory::Security,
            "source_access_denied".into(),
        ),
        AgentFailure::PolicyDenied => (
            AgentFailureDomain::App,
            AgentFailureCategory::Internal,
            "internal_policy_invariant".into(),
        ),
        AgentFailure::CapabilityDenied => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::Security,
            "capability_access_denied".into(),
        ),
        AgentFailure::AccessReviewRequired => (
            AgentFailureDomain::Source,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::ConsentRequired => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::CapabilityUnavailable => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::Conflict | AgentFailure::StaleContext if source_stage => (
            AgentFailureDomain::Source,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::ServerModelRequestRejected
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => (
            AgentFailureDomain::Capability,
            if matches!(
                failure,
                AgentFailure::InvalidModelOutput
                    | AgentFailure::LocalModelInvalidOutput
                    | AgentFailure::ServerModelInvalidOutput
            ) {
                AgentFailureCategory::Integrity
            } else {
                AgentFailureCategory::Transient
            },
            reason_code.clone(),
        ),
        AgentFailure::CredentialExpired | AgentFailure::QuotaExceeded => (
            AgentFailureDomain::Capability,
            AgentFailureCategory::UserConfiguration,
            reason_code.clone(),
        ),
        AgentFailure::Interrupted | AgentFailure::DeadlineExceeded | AgentFailure::Stalled => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Transient,
            reason_code.clone(),
        ),
        _ if stage == "conversation_turn" => (
            AgentFailureDomain::Turn,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        _ if stage == "conversation_session" => (
            AgentFailureDomain::Session,
            AgentFailureCategory::Integrity,
            reason_code.clone(),
        ),
        _ => (
            AgentFailureDomain::App,
            AgentFailureCategory::Internal,
            reason_code,
        ),
    };

    let mut safe_actions = match failure {
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => {
            vec![AgentFailureSafeAction::ReopenVault]
        }
        _ if stage == "conversation_session" => {
            vec![AgentFailureSafeAction::StartNewSession]
        }
        AgentFailure::PolicyDenied if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ExportDiagnostics,
        ],
        AgentFailure::PolicyDenied if source_stage => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ReviewSource,
        ],
        AgentFailure::AccessReviewRequired => vec![
            AgentFailureSafeAction::ContinueWithoutSource,
            AgentFailureSafeAction::ReviewSource,
        ],
        AgentFailure::ConsentRequired => vec![],
        AgentFailure::Conflict if stage == "conversation_session" => vec![
            AgentFailureSafeAction::StartNewSession,
            AgentFailureSafeAction::RefreshSession,
        ],
        AgentFailure::Conflict if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::RefreshSession,
            AgentFailureSafeAction::StartNewSession,
        ],
        AgentFailure::StaleContext if stage == "conversation_turn" => vec![
            AgentFailureSafeAction::RefreshSession,
            AgentFailureSafeAction::StartNewSession,
        ],
        AgentFailure::StaleContext => vec![AgentFailureSafeAction::ReviewSource],
        AgentFailure::CredentialExpired => vec![AgentFailureSafeAction::RefreshSession],
        AgentFailure::CapabilityUnavailable if source_stage => {
            vec![AgentFailureSafeAction::ContinueWithoutSource]
        }
        AgentFailure::Cancelled => vec![],
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => vec![AgentFailureSafeAction::Retry],
        _ if stage == "conversation_turn" && !matches!(failure, AgentFailure::Cancelled) => {
            vec![AgentFailureSafeAction::StartNewSession]
        }
        _ => vec![],
    };
    if !matches!(failure, AgentFailure::Cancelled)
        && matches!(
            category,
            AgentFailureCategory::Internal | AgentFailureCategory::Security
        )
        && !safe_actions.contains(&AgentFailureSafeAction::ExportDiagnostics)
    {
        safe_actions.push(AgentFailureSafeAction::ExportDiagnostics);
    }
    let retry_policy = if safe_actions.contains(&AgentFailureSafeAction::Retry) {
        if matches!(
            failure,
            AgentFailure::ServerModelTimeout
                | AgentFailure::Interrupted
                | AgentFailure::DeadlineExceeded
                | AgentFailure::Stalled
        ) {
            AgentRetryPolicy::Backoff
        } else {
            AgentRetryPolicy::Immediate
        }
    } else {
        AgentRetryPolicy::Never
    };
    let retryable = !matches!(retry_policy, AgentRetryPolicy::Never);
    FailureClassification {
        domain,
        category,
        reason_code,
        safe_actions,
        retry_policy,
        retryable,
    }
}

fn reload_required(recovery: AgentVaultRecoveryActionDto) -> bool {
    !matches!(
        recovery,
        AgentVaultRecoveryActionDto::RetryRead
            | AgentVaultRecoveryActionDto::ReviewSource
            | AgentVaultRecoveryActionDto::RefreshContext
    )
}

fn seal_session(failure: &AgentFailure, recovery: AgentVaultRecoveryActionDto) -> bool {
    matches!(recovery, AgentVaultRecoveryActionDto::ReopenVault)
        || matches!(
            failure,
            AgentFailure::VaultUnavailable
                | AgentFailure::StorageUnavailable
                | AgentFailure::Interrupted
        )
}

fn recovery_action(failure: &AgentFailure, stage: &str) -> AgentVaultRecoveryActionDto {
    match failure {
        AgentFailure::Conflict | AgentFailure::DeadlineExceeded | AgentFailure::Interrupted
            if stage == "calendar_action" =>
        {
            AgentVaultRecoveryActionDto::Reconcile
        }
        AgentFailure::Conflict if matches!(stage, "conversation_session" | "conversation_turn") => {
            AgentVaultRecoveryActionDto::RefreshSession
        }
        AgentFailure::Conflict | AgentFailure::StaleContext => {
            AgentVaultRecoveryActionDto::RefreshContext
        }
        AgentFailure::ModelUnavailable
        | AgentFailure::LocalModelUnavailable
        | AgentFailure::ServerModelUnavailable
        | AgentFailure::ServerModelTimeout
        | AgentFailure::InvalidModelOutput
        | AgentFailure::LocalModelInvalidOutput
        | AgentFailure::ServerModelInvalidOutput => AgentVaultRecoveryActionDto::RetryRead,
        AgentFailure::AccessReviewRequired
            if matches!(
                stage,
                "calendar_access"
                    | "calendar_experts"
                    | "calendar_subject_preview"
                    | "calendar_action"
                    | "personal_access"
                    | "contacts_access"
                    | "conversation_turn"
            ) =>
        {
            AgentVaultRecoveryActionDto::ReviewSource
        }
        AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable => {
            AgentVaultRecoveryActionDto::ReopenVault
        }
        _ => AgentVaultRecoveryActionDto::None,
    }
}

fn feasibility_query(query: &FeasibilityGrantQueryDto) -> FeasibilityGrantQuery {
    let mut evidence_handles = query.evidence_handles.clone();
    evidence_handles.sort();
    FeasibilityGrantQuery {
        event_handle: query.event_handle.clone(),
        evidence_handles,
        destination_latitude: query.destination_latitude,
        destination_longitude: query.destination_longitude,
        event_start_unix_ms: query.event_start_unix_ms,
        event_end_unix_ms: query.event_end_unix_ms,
        travel_mode: query.travel_mode.clone(),
    }
}

fn personal_access(request: &PersonalAccessConfigurationDto) -> PersonalAccessConfiguration {
    PersonalAccessConfiguration {
        connector: request.connector.clone(),
        device_id: request.device_id.clone(),
        change: personal_access_change(&request.change),
    }
}

pub(crate) fn personal_access_change(change: &PersonalAccessChangeDto) -> PersonalAccessChange {
    match change {
        PersonalAccessChangeDto::Inspect {} => PersonalAccessChange::Inspect,
        PersonalAccessChangeDto::Review {
            expected_native_subject_fingerprint,
            consumers,
            feasibility_query: query,
            expected_grant_id,
            expected_grant_authority,
        } => PersonalAccessChange::Review {
            expected_native_subject_fingerprint: expected_native_subject_fingerprint.clone(),
            consumers: consumers.clone(),
            feasibility_query: query.as_ref().map(feasibility_query),
            expected_grant_id: *expected_grant_id,
            expected_grant_authority: *expected_grant_authority,
        },
        PersonalAccessChangeDto::SetEnabled { enabled } => {
            PersonalAccessChange::SetEnabled { enabled: *enabled }
        }
    }
}

fn contacts_access(request: &ContactsAccessConfigurationDto) -> ContactsAccessConfiguration {
    ContactsAccessConfiguration {
        connector: request.connector.clone(),
        device_id: request.device_id.clone(),
        change: contacts_access_change(&request.change),
    }
}

pub(crate) fn contacts_access_change(change: &ContactsAccessChangeDto) -> ContactsAccessChange {
    match change {
        ContactsAccessChangeDto::Inspect { selected_handles } => ContactsAccessChange::Inspect {
            selected_handles: selected_handles.clone(),
        },
        ContactsAccessChangeDto::Review {
            selected_handles,
            expected_native_subject_fingerprint,
            consumers,
            expected_grant_id,
            expected_grant_authority,
        } => ContactsAccessChange::Review {
            selected_handles: selected_handles.clone(),
            expected_native_subject_fingerprint: expected_native_subject_fingerprint.clone(),
            consumers: consumers.clone(),
            expected_grant_id: *expected_grant_id,
            expected_grant_authority: *expected_grant_authority,
        },
        ContactsAccessChangeDto::SetEnabled { enabled } => {
            ContactsAccessChange::SetEnabled { enabled: *enabled }
        }
    }
}

pub(crate) fn producer_identity_dto(
    identity: &RemoteProducerIdentity,
) -> RemoteProducerIdentityDto {
    RemoteProducerIdentityDto {
        schema_version: identity.schema_version,
        instance_id: identity.instance_id.clone(),
        execution_owner: identity.execution_owner.clone(),
        audience: identity.audience.clone(),
        key_id: identity.key_id.clone(),
        public_key: identity.public_key.clone(),
        fingerprint: identity.fingerprint.clone(),
    }
}

pub(crate) fn producer_identity(identity: &RemoteProducerIdentityDto) -> RemoteProducerIdentity {
    RemoteProducerIdentity {
        schema_version: identity.schema_version,
        instance_id: identity.instance_id.clone(),
        execution_owner: identity.execution_owner.clone(),
        audience: identity.audience.clone(),
        key_id: identity.key_id.clone(),
        public_key: identity.public_key.clone(),
        fingerprint: identity.fingerprint.clone(),
    }
}

pub(crate) fn owner_key_dto(key: &RemoteOwnerPublicKey) -> RemoteOwnerPublicKeyDto {
    RemoteOwnerPublicKeyDto {
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        fingerprint: key.fingerprint(),
    }
}

pub(crate) fn pairing_issuer_dto(issuer: &PairingIssuer) -> RemoteOwnerPublicKeyDto {
    RemoteOwnerPublicKeyDto {
        key_id: issuer.key_id.clone(),
        public_key: issuer.public_key.clone(),
        fingerprint: issuer.fingerprint.clone(),
    }
}

pub(crate) fn enrollment_status_dto(
    status: RemoteEnrollmentStatus,
) -> RemoteAuthorityEnrollmentStatusDto {
    RemoteAuthorityEnrollmentStatusDto {
        enrollment_id: status.enrollment_id,
        key_id: status.key_id,
        fingerprint: status.fingerprint,
        local_confirmed: status.local_confirmed,
        admin_approved: status.admin_approved,
        active: status.active,
    }
}

pub(crate) fn vault_state_dto(state: VaultState) -> AgentVaultStateDto {
    match state {
        VaultState::Missing => AgentVaultStateDto::Missing,
        VaultState::Locked => AgentVaultStateDto::Locked,
        VaultState::Ready => AgentVaultStateDto::Ready,
        VaultState::Unavailable => AgentVaultStateDto::Unavailable,
    }
}

pub(crate) fn subject_preview_dto(preview: CalendarSubjectPreview) -> CalendarSubjectPreviewDto {
    CalendarSubjectPreviewDto {
        provider: super::day::calendar_provider_to_dto(preview.provider),
        device_id: preview.device_id,
        calendar_ids: preview.calendar_ids,
        connection_scope: super::day::calendar_scope_to_dto(preview.connection_scope),
        connection_id: preview.connection_id,
        connection_revision: preview.connection_revision,
        source_authority: preview.source_authority,
        native_subject_fingerprint: preview.native_subject_fingerprint,
    }
}

pub(crate) fn proposal_dto(proposal: CalendarProposalInspection) -> AgentProposalInspectionDto {
    AgentProposalInspectionDto {
        schema_version: PROTOCOL_VERSION,
        person_id: proposal.person_id.to_string(),
        session_id: proposal.session_id.to_string(),
        invocation_id: proposal.invocation_id.to_string(),
        action: proposal.action.map(calendar_action),
    }
}

pub(crate) fn memory_review_dto(
    review: MemoryReviewResult,
) -> Result<AgentMemoryReviewOverviewDto, AgentFailure> {
    Ok(AgentMemoryReviewOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: review.snapshot.person_id.to_string(),
        candidates: review
            .snapshot
            .candidates
            .iter()
            .map(encode_contract)
            .collect::<Result<Vec<_>, _>>()?,
        decision: review.decision.as_ref().map(encode_contract).transpose()?,
    })
}

pub(crate) fn memory_dto(
    snapshot: floe_app::MemoryOverviewSnapshot,
) -> Result<AgentMemoryOverviewDto, AgentFailure> {
    Ok(AgentMemoryOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: snapshot.person_id.to_string(),
        saved_count: snapshot.saved_count,
        pending_count: snapshot.pending_count,
        memories: snapshot
            .memories
            .into_iter()
            .map(|memory| {
                Ok::<_, AgentFailure>(AgentMemorySummaryDto {
                    target_id: memory.target_id.to_string(),
                    revision: memory.revision,
                    statement: memory.statement,
                    memory_kind: encode_contract(&memory.memory_kind)?,
                    epistemic_status: encode_contract(&memory.epistemic_status)?,
                    confidence_millis: memory.confidence_millis,
                    source_count: memory.source_count,
                    origin: match memory.origin {
                        floe_app::MemoryOrigin::UserProvided => AgentMemoryOriginDto::UserProvided,
                        floe_app::MemoryOrigin::Learned => AgentMemoryOriginDto::Learned,
                    },
                    created_at: memory.created_at,
                    valid_from: memory.valid_from,
                    valid_until: memory.valid_until,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

pub(crate) fn remote_calendar_preview_dto(
    person_id: PersonId,
    preview: RemoteCalendarGrantPreview,
) -> RemoteCalendarGrantPreviewDto {
    RemoteCalendarGrantPreviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: person_id.to_string(),
        connector_id: preview.connector_id,
        connection_id: preview.connection_id,
        resource: preview.resource,
        source_authority: preview.source_authority,
        provider_identity: preview.provider_identity,
        execution_owner: preview.execution_owner,
        producer: producer_identity_dto(&preview.producer),
        consumer: preview.consumer,
        purpose: "everyday_assistance".into(),
        recipient: preview.recipient,
    }
}

/// Report one command's outcome on the wire.
pub fn worker_result(result: WorkerResult) -> Result<AgentVaultResultDto, AgentFailure> {
    let request_id = result.request_id.to_string();
    // A halted Session reports its own reason as the request's failure, so a
    // caller is never told a turn succeeded when its outcome says otherwise.
    let failure = result.failure.as_ref().or_else(|| {
        match result
            .session
            .as_ref()
            .and_then(|session| session.last_outcome.as_ref())
        {
            Some(floe_app::AgentOutcome::Halted { reason }) => Some(reason),
            _ => None,
        }
    });
    Ok(AgentVaultResultDto {
        events: encode_contracts(result.events.clone())?,
        next_sequence: result.next_sequence,
        done: result.done,
        state: result.state.map(vault_state_dto),
        session: result.session.as_ref().map(encode_contract).transpose()?,
        registry: result.registry.as_ref().map(encode_contract).transpose()?,
        calendar_experts: result
            .calendar_experts
            .as_ref()
            .map(encode_contract)
            .transpose()?,
        calendar_subject_preview: result.calendar_subject_preview.map(subject_preview_dto),
        proposal: result.proposal.map(proposal_dto),
        memory_review: result.memory_review.map(memory_review_dto).transpose()?,
        memory: result.memory.map(memory_dto).transpose()?,
        connections: result.connections.map(encode_contracts).transpose()?,
        personal_access: result.personal_access.map(personal_access_dto),
        calendar_actions: result
            .calendar_actions
            .as_ref()
            .map(|actions| serde_json::to_value(actions).map_err(|_| AgentFailure::InvalidInput))
            .transpose()?,
        failure: failure.map(|failure| failure_envelope(failure, &result.stage, &request_id)),
        request_id,
    })
}

pub(crate) fn personal_access_dto(
    overview: floe_app::PersonalAccessOverview,
) -> PersonalAccessOverviewDto {
    PersonalAccessOverviewDto {
        schema_version: 1,
        person_id: overview.person_id.to_string(),
        connector: overview.connector,
        device_id: overview.device_id,
        connection_id: overview.connection_id,
        source_authority: overview.source_authority,
        grant_id: overview.grant_id,
        grant_authority: overview.grant_authority,
        state: match overview.state {
            floe_app::PersonalAccessState::NeedsReview => "needs_review".into(),
            floe_app::PersonalAccessState::Paused => "paused".into(),
            floe_app::PersonalAccessState::Active => "active".into(),
            floe_app::PersonalAccessState::Revoked => "revoked".into(),
        },
        review_required: overview.review_required,
        presence_available: overview.presence_available,
        consumers: overview.consumers,
        native_subject_fingerprint: overview.native_subject_fingerprint,
        process_incarnation: overview.process_incarnation,
    }
}

fn fixture_prompt(prompt: AgentFixturePromptDto) -> floe_app::AgentFixturePrompt {
    match prompt {
        AgentFixturePromptDto::Today => floe_app::AgentFixturePrompt::Today,
        AgentFixturePromptDto::FollowUp => floe_app::AgentFixturePrompt::FollowUp,
        AgentFixturePromptDto::RepeatedCall => floe_app::AgentFixturePrompt::RepeatedCall,
        AgentFixturePromptDto::Unavailable => floe_app::AgentFixturePrompt::Unavailable,
    }
}

fn fixture_operation(operation: AgentFixtureOperationDto) -> WireResult<FixtureOperation> {
    Ok(match operation {
        AgentFixtureOperationDto::Start {} => FixtureOperation::Start,
        AgentFixtureOperationDto::Resume {} => FixtureOperation::Resume,
        AgentFixtureOperationDto::Get { session_id } => FixtureOperation::Get {
            session_id: parse_uuid(&session_id, "operation.session_id")?,
        },
        AgentFixtureOperationDto::Turn {
            session_id,
            expected_revision,
            prompt,
        } => FixtureOperation::Turn {
            session_id: parse_uuid(&session_id, "operation.session_id")?,
            expected_revision,
            prompt: fixture_prompt(prompt),
        },
        AgentFixtureOperationDto::Recover {
            session_id,
            expected_revision,
        } => FixtureOperation::Recover {
            session_id: parse_uuid(&session_id, "operation.session_id")?,
            expected_revision,
        },
    })
}

fn session_operation(
    operation: AgentConversationSessionOperationDto,
) -> WireResult<ConversationSessionOperation> {
    Ok(match operation {
        AgentConversationSessionOperationDto::Start {} => ConversationSessionOperation::Start,
        AgentConversationSessionOperationDto::Resume {} => ConversationSessionOperation::Resume,
        AgentConversationSessionOperationDto::Get { session_id } => {
            ConversationSessionOperation::Get {
                session_id: parse_uuid(&session_id, "operation.session_id")?,
            }
        }
        AgentConversationSessionOperationDto::Recover {
            session_id,
            expected_revision,
        } => ConversationSessionOperation::Recover {
            session_id: parse_uuid(&session_id, "operation.session_id")?,
            expected_revision,
        },
    })
}

fn proposal(
    calendar_id: String,
    title: String,
    starts_at: String,
    ends_at: String,
    timezone: String,
    event_id: Option<String>,
    event_revision: Option<u64>,
    delete: bool,
) -> Box<CalendarActionProposal> {
    Box::new(CalendarActionProposal {
        calendar_id,
        title,
        starts_at,
        ends_at,
        timezone,
        event_id,
        event_revision,
        delete,
    })
}

pub(crate) fn calendar_action_operation(
    operation: CalendarActionOperationDto,
) -> WireResult<CalendarActionOperation> {
    Ok(match operation {
        CalendarActionOperationDto::Capabilities {} => CalendarActionOperation::Capabilities,
        CalendarActionOperationDto::GetAuthority {} => CalendarActionOperation::GetAuthority,
        CalendarActionOperationDto::SetAuthority { calendar_create } => {
            CalendarActionOperation::SetAuthority {
                calendar_create: match calendar_create {
                    ActionAuthorityModeDto::Allow => floe_app::ActionAuthorityMode::Allow,
                    ActionAuthorityModeDto::Ask => floe_app::ActionAuthorityMode::Ask,
                    ActionAuthorityModeDto::Deny => floe_app::ActionAuthorityMode::Deny,
                },
            }
        }
        CalendarActionOperationDto::Execute { action_id } => CalendarActionOperation::Execute {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
        },
        CalendarActionOperationDto::Recover { action_id } => CalendarActionOperation::Recover {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
        },
        CalendarActionOperationDto::List {} => CalendarActionOperation::List,
        CalendarActionOperationDto::Get { action_id } => CalendarActionOperation::Get {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
        },
        CalendarActionOperationDto::Propose {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
        } => CalendarActionOperation::Propose(proposal(
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            None,
            None,
            false,
        )),
        CalendarActionOperationDto::Direct {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            event_id,
            event_revision,
            delete,
        } => CalendarActionOperation::Direct(proposal(
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            event_id,
            event_revision,
            delete,
        )),
        CalendarActionOperationDto::Decide {
            action_id,
            decision,
        } => CalendarActionOperation::Decide {
            action_id: parse_uuid(&action_id, "operation.action_id")?,
            approve: decision == CalendarActionDecisionDto::Approve,
        },
    })
}

fn subject_request(request: CalendarSubjectPreviewRequestDto) -> CalendarSubjectRequest {
    CalendarSubjectRequest {
        provider: super::day::calendar_provider_from_dto(request.provider),
        device_id: request.device_id,
        connection_id: request.connection_id,
        calendar_ids: request.calendar_ids,
        connection_scope: super::day::calendar_scope_from_dto(request.connection_scope),
        connection_revision: request.connection_revision,
        source_authority: request.source_authority,
    }
}

/// Read one worker request off the wire as the app's own command.
pub fn worker_operation(operation: AgentVaultOperationDto) -> WireResult<WorkerOperation> {
    Ok(match operation {
        AgentVaultOperationDto::Submit { action } => WorkerOperation::Submit {
            action: Box::new(worker_action(action)?),
        },
        AgentVaultOperationDto::Poll { after_sequence } => WorkerOperation::Poll { after_sequence },
        AgentVaultOperationDto::Stop {} => WorkerOperation::Stop,
        AgentVaultOperationDto::Release {} => WorkerOperation::Release,
    })
}

fn worker_action(action: AgentVaultActionDto) -> WireResult<WorkerAction> {
    Ok(match action {
        AgentVaultActionDto::Status {} => WorkerAction::Status,
        AgentVaultActionDto::Create {} => WorkerAction::Create,
        AgentVaultActionDto::Unlock {} => WorkerAction::Unlock,
        AgentVaultActionDto::Lock {} => WorkerAction::Lock,
        AgentVaultActionDto::Session { operation } => WorkerAction::Session {
            operation: fixture_operation(operation)?,
        },
        AgentVaultActionDto::Registry { change } => WorkerAction::Registry {
            change: change
                .as_ref()
                .map(decode_contract)
                .transpose()
                .map_err(|_| invalid("action.change", "invalid registry configuration"))?,
        },
        AgentVaultActionDto::CalendarExperts { setup } => WorkerAction::CalendarExperts {
            setup: setup
                .as_ref()
                .map(decode_contract)
                .transpose()
                .map_err(|_| invalid("action.setup", "invalid calendar expert setup"))?
                .map(Box::new),
        },
        AgentVaultActionDto::CalendarAccess { change } => WorkerAction::CalendarAccess {
            change: Box::new(
                decode_contract(&change)
                    .map_err(|_| invalid("action.change", "invalid calendar access change"))?,
            ),
        },
        AgentVaultActionDto::CalendarSubjectPreview { request } => {
            WorkerAction::CalendarSubjectPreview {
                request: Box::new(subject_request(request)),
            }
        }
        AgentVaultActionDto::PersonalAccess { change } => WorkerAction::PersonalAccess {
            change: Box::new(personal_access(&change)),
        },
        AgentVaultActionDto::ContactsAccess { change } => WorkerAction::ContactsAccess {
            change: Box::new(contacts_access(&change)),
        },
        AgentVaultActionDto::CalendarAction { operation } => WorkerAction::CalendarAction {
            operation: calendar_action_operation(operation)?,
        },
        AgentVaultActionDto::InspectProposal {
            session_id,
            invocation_id,
        } => WorkerAction::InspectProposal {
            session_id: parse_uuid(&session_id, "action.session_id")?,
            invocation_id: parse_uuid(&invocation_id, "action.invocation_id")?,
        },
        AgentVaultActionDto::ConversationSession { operation } => {
            WorkerAction::ConversationSession {
                operation: session_operation(operation)?,
            }
        }
        AgentVaultActionDto::MemoryReview { decision } => WorkerAction::MemoryReview {
            decision: decision
                .map(|decision| {
                    Ok::<_, floe_protocol::ErrorDto>(MemoryReviewDecision {
                        candidate_id: parse_uuid(&decision.candidate_id, "action.candidate_id")?,
                        kind: match decision.decision {
                            AgentMemoryReviewDecisionKindDto::Approve => {
                                floe_app::KnowledgeDecisionKind::Approve
                            }
                            AgentMemoryReviewDecisionKindDto::Reject => {
                                floe_app::KnowledgeDecisionKind::Reject
                            }
                        },
                    })
                })
                .transpose()?,
        },
        AgentVaultActionDto::Memory {} => WorkerAction::Memory,
        AgentVaultActionDto::Connections {} => WorkerAction::Connections,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_app::AgentFailure;

    #[test]
    fn a_candidate_id_that_is_not_an_id_never_reaches_the_worker() {
        let error = worker_action(AgentVaultActionDto::MemoryReview {
            decision: Some(AgentMemoryReviewDecisionDto {
                candidate_id: "invalid".into(),
                decision: AgentMemoryReviewDecisionKindDto::Approve,
            }),
        })
        .err()
        .expect("an unparseable candidate id is rejected on the wire");
        assert_eq!(error.code, ErrorCodeDto::Validation);
        assert_eq!(error.field.as_deref(), Some("action.candidate_id"));
    }

    #[test]
    fn failure_recovery_is_stage_aware_and_conservative() {
        let conversation =
            failure_envelope(&AgentFailure::Conflict, "conversation_turn", "request");
        assert_eq!(
            conversation.recovery_action,
            AgentVaultRecoveryActionDto::RefreshSession
        );
        assert!(!conversation.retryable);
        assert_eq!(conversation.domain, AgentFailureDomain::Turn);
        assert_eq!(conversation.category, AgentFailureCategory::Integrity);
        assert_eq!(conversation.reason_code, "conflict");
        assert!(
            conversation
                .safe_actions
                .contains(&AgentFailureSafeAction::StartNewSession)
        );

        let session_policy = failure_envelope(
            &AgentFailure::PolicyDenied,
            "conversation_session",
            "request",
        );
        assert_eq!(session_policy.domain, AgentFailureDomain::Session);
        assert_eq!(session_policy.category, AgentFailureCategory::Integrity);
        assert_eq!(session_policy.reason_code, "session_integrity");
        assert_eq!(
            session_policy.safe_actions,
            vec![AgentFailureSafeAction::StartNewSession]
        );

        let release_block =
            failure_envelope(&AgentFailure::PolicyDenied, "conversation_turn", "request");
        assert_eq!(release_block.domain, AgentFailureDomain::Turn);
        assert_eq!(release_block.category, AgentFailureCategory::Security);
        assert_eq!(release_block.reason_code, "data_release_or_policy_block");
        assert!(
            release_block
                .safe_actions
                .contains(&AgentFailureSafeAction::ContinueWithoutSource)
        );
        assert!(
            release_block
                .safe_actions
                .contains(&AgentFailureSafeAction::ExportDiagnostics)
        );

        let model_refusal = failure_envelope(
            &AgentFailure::CapabilityDenied,
            "conversation_turn",
            "request",
        );
        assert_eq!(model_refusal.reason_code, "capability_access_denied");
        assert_eq!(model_refusal.category, AgentFailureCategory::Security);
        assert!(
            model_refusal
                .safe_actions
                .contains(&AgentFailureSafeAction::ExportDiagnostics)
        );

        let model_retry = failure_envelope(
            &AgentFailure::ServerModelTimeout,
            "conversation_turn",
            "request",
        );
        assert_eq!(model_retry.retry_policy, AgentRetryPolicy::Backoff);
        assert!(model_retry.retryable);
        assert!(
            model_retry
                .safe_actions
                .contains(&AgentFailureSafeAction::Retry)
        );

        let setup = failure_envelope(&AgentFailure::Conflict, "calendar_access", "request");
        assert_eq!(
            setup.recovery_action,
            AgentVaultRecoveryActionDto::RefreshContext
        );

        let review = failure_envelope(
            &AgentFailure::AccessReviewRequired,
            "calendar_access",
            "request",
        );
        assert_eq!(
            review.recovery_action,
            AgentVaultRecoveryActionDto::ReviewSource
        );
        let turn_review = failure_envelope(
            &AgentFailure::AccessReviewRequired,
            "conversation_turn",
            "request",
        );
        assert_eq!(
            turn_review.recovery_action,
            AgentVaultRecoveryActionDto::ReviewSource
        );
        let model_consent = failure_envelope(
            &AgentFailure::ConsentRequired,
            "conversation_turn",
            "request",
        );
        assert_eq!(model_consent.domain, AgentFailureDomain::Capability);
        assert_eq!(
            model_consent.category,
            AgentFailureCategory::UserConfiguration
        );
        assert!(model_consent.safe_actions.is_empty());
        assert_eq!(
            model_consent.recovery_action,
            AgentVaultRecoveryActionDto::None
        );

        for failure in [
            AgentFailure::CapabilityUnavailable,
            AgentFailure::Cancelled,
            AgentFailure::Interrupted,
            AgentFailure::DeadlineExceeded,
        ] {
            let envelope = failure_envelope(&failure, "calendar_experts", "request");
            assert_eq!(envelope.recovery_action, AgentVaultRecoveryActionDto::None);
            assert!(!envelope.retryable);
        }
    }
}
