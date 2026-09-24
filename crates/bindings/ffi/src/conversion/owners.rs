use floe_app::{
    AgentFailure, CalendarActionOperation, CalendarActionProposal, CalendarActionState,
    CalendarProposalInspection, CalendarSubjectPreview, ContactsAccessChange,
    FeasibilityGrantQuery, GrantState, MemoryReviewResult, PairingIssuer, PersonId,
    PersonalAccessChange, ProcessingRestriction, RemoteCalendarGrantPreview,
    RemoteEnrollmentStatus, RemoteOwnerPublicKey, RemoteProducerIdentity, VaultState,
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
    let consumers = grant
        .scope()
        .consumers()
        .iter()
        .map(|consumer| consumer.identifier().to_owned())
        .collect();
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
        consumers,
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
        consumer: preview.consumers.first().cloned().unwrap_or_default(),
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

fn encode_contract<T: DeserializeOwned>(value: &impl Serialize) -> Result<T, AgentFailure> {
    let value = serde_json::to_value(value).map_err(|_| AgentFailure::InvalidModelOutput)?;
    serde_json::from_value(value).map_err(|_| AgentFailure::InvalidModelOutput)
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
        "calendar_subject_preview"
            | "calendar_access"
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
        | AgentFailure::ServerModelInvalidOutput
            if stage == "conversation_session" =>
        {
            AgentVaultRecoveryActionDto::None
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
                "calendar_subject_preview"
                    | "calendar_access"
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

pub(crate) fn personal_access_change(change: &PersonalAccessChangeDto) -> PersonalAccessChange {
    match change {
        PersonalAccessChangeDto::Inspect {} => PersonalAccessChange::Inspect,
        PersonalAccessChangeDto::Review {
            expected_native_subject_fingerprint,
            feasibility_query: query,
            expected_grant_id,
            expected_grant_authority,
        } => PersonalAccessChange::Review {
            expected_native_subject_fingerprint: expected_native_subject_fingerprint.clone(),
            feasibility_query: query.as_ref().map(feasibility_query),
            expected_grant_id: *expected_grant_id,
            expected_grant_authority: *expected_grant_authority,
        },
        PersonalAccessChangeDto::SetEnabled { enabled } => {
            PersonalAccessChange::SetEnabled { enabled: *enabled }
        }
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
            expected_grant_id,
            expected_grant_authority,
        } => ContactsAccessChange::Review {
            selected_handles: selected_handles.clone(),
            expected_native_subject_fingerprint: expected_native_subject_fingerprint.clone(),
            expected_grant_id: *expected_grant_id,
            expected_grant_authority: *expected_grant_authority,
        },
        ContactsAccessChangeDto::SetEnabled { enabled } => {
            ContactsAccessChange::SetEnabled { enabled: *enabled }
        }
    }
}

pub(crate) fn calendar_access_change(
    change: &CalendarAccessChangeDto,
) -> floe_app::CalendarAccessChange {
    match change {
        CalendarAccessChangeDto::Review {
            connection_id,
            calendar_ids,
            expected_source_authority,
            expected_native_subject_fingerprint,
            expected_grant_id,
            expected_grant_authority,
        } => floe_app::CalendarAccessChange::Review {
            connection_id: connection_id.clone(),
            calendar_ids: calendar_ids.clone(),
            expected_source_authority: *expected_source_authority,
            expected_native_subject_fingerprint: expected_native_subject_fingerprint.clone(),
            expected_grant_id: *expected_grant_id,
            expected_grant_authority: *expected_grant_authority,
        },
        CalendarAccessChangeDto::Pause {
            grant_id,
            expected_grant_authority,
        } => floe_app::CalendarAccessChange::Pause {
            grant_id: *grant_id,
            expected_grant_authority: *expected_grant_authority,
        },
        CalendarAccessChangeDto::Remove {
            grant_id,
            expected_grant_authority,
        } => floe_app::CalendarAccessChange::Remove {
            grant_id: *grant_id,
            expected_grant_authority: *expected_grant_authority,
        },
    }
}

pub(crate) fn calendar_access_dto(
    overview: floe_app::CalendarAccessOverview,
) -> CalendarAccessOverviewDto {
    CalendarAccessOverviewDto {
        schema_version: PROTOCOL_VERSION,
        person_id: overview.person_id.to_string(),
        provider: super::day::calendar_provider_to_dto(overview.provider),
        connection_id: overview.connection_id,
        selected_resources: overview.selected_resources,
        granted_resources: overview.granted_resources,
        source_authority: overview.source_authority,
        grant_id: overview.grant_id,
        grant_authority: overview.grant_authority,
        consumer_policy: overview.consumer_policy,
        state: match overview.state {
            floe_app::CalendarAccessState::NeedsReview => "needs_review".into(),
            floe_app::CalendarAccessState::Paused => "paused".into(),
            floe_app::CalendarAccessState::Active => "active".into(),
            floe_app::CalendarAccessState::Revoked => "revoked".into(),
        },
        review_required: overview.review_required,
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
        consumers: preview.consumers,
        purpose: "everyday_assistance".into(),
        recipient: preview.recipient,
        grant_id: preview.grant_id,
        grant_authority: preview.grant_authority,
        consumer_policy: preview.consumer_policy,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use floe_app::AgentFailure;

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

        let failed_session = failure_envelope(
            &AgentFailure::LocalModelInvalidOutput,
            "conversation_session",
            "request",
        );
        assert_eq!(
            failed_session.recovery_action,
            AgentVaultRecoveryActionDto::None
        );
        assert_eq!(failed_session.retry_policy, AgentRetryPolicy::Never);
        assert!(!failed_session.retryable);
        assert_eq!(
            failed_session.safe_actions,
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

        let setup = failure_envelope(&AgentFailure::Conflict, "registry", "request");
        assert_eq!(
            setup.recovery_action,
            AgentVaultRecoveryActionDto::RefreshContext
        );

        let review = failure_envelope(
            &AgentFailure::AccessReviewRequired,
            "calendar_action",
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
            let envelope = failure_envelope(&failure, "registry", "request");
            assert_eq!(envelope.recovery_action, AgentVaultRecoveryActionDto::None);
            assert!(!envelope.retryable);
        }
    }
}
