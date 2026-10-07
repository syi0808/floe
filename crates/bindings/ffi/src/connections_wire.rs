//! Mechanical conversion between the Connections owner and its safe product wire.
use super::app_wire::{
    AppCommandFailure, AppCommandResult, AppWireResult, agent_failure, internal_error, validation,
};
use floe_connections as owner;
use floe_execution::ExecutionScope;
use floe_kernel::OwnerActor;
use floe_protocol as dto;
use uuid::Uuid;

pub(crate) fn handles_command(command: &dto::AppProductCommandDto) -> bool {
    use dto::AppProductCommandDto::*;
    matches!(
        command,
        ConnectionsPairingStart { .. }
            | ConnectionsPairingCancel { .. }
            | ConnectionsGatewayForget { .. }
            | ConnectionsIntegrationPrepareReview { .. }
            | ConnectionsIntegrationStart { .. }
            | ConnectionsOperationCancel { .. }
            | ConnectionsSourcePrepareReview { .. }
            | ConnectionsSourceConfigure { .. }
            | ConnectionsDisconnect { .. }
            | ConnectionsObservePrepareReview { .. }
            | ConnectionsObserveSet { .. }
            | ConnectionsGatewayManagementLaunch { .. }
    )
}
pub(crate) fn handles_query(query: &dto::AppProductQueryDto) -> bool {
    use dto::AppProductQueryDto::*;
    matches!(
        query,
        ConnectionsOverview { .. }
            | ConnectionsPairingGet { .. }
            | ConnectionsGatewayGet { .. }
            | ConnectionsIntegrationInspectReview { .. }
            | ConnectionsOperationGet { .. }
            | ConnectionsSourceInspectReview { .. }
            | ConnectionsObserveInspectReview { .. }
    )
}

fn command_failure(failure: owner::ConnectionsCommandFailure) -> AppCommandFailure {
    match failure {
        owner::ConnectionsCommandFailure::NotApplied(reason) => {
            AppCommandFailure::NotApplied(agent_failure(reason))
        }
        owner::ConnectionsCommandFailure::NotAdmitted(reason) => {
            AppCommandFailure::NotAdmitted(agent_failure(reason))
        }
        owner::ConnectionsCommandFailure::Admitted(reason) => {
            AppCommandFailure::Admitted(agent_failure(reason))
        }
        owner::ConnectionsCommandFailure::Indeterminate(reason) => {
            AppCommandFailure::Indeterminate(agent_failure(reason))
        }
    }
}

pub(crate) async fn command(
    owners: &floe_app::ReadyOwners,
    actor: &OwnerActor,
    command_id: Uuid,
    command: dto::AppProductCommandDto,
    scope: &ExecutionScope,
) -> AppCommandResult<dto::AppCommandResultDto> {
    use dto::{AppCommandResultDto as R, AppProductCommandDto as C};
    let service = &owners.connections;
    Ok(match command {
        C::ConnectionsPairingStart { address_text } => R::ConnectionsPairing {
            pairing: pairing(
                service
                    .start_pairing(actor, command_id, &address_text, scope)
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsPairingCancel {
            operation_ref,
            expected_revision,
        } => R::ConnectionsPairing {
            pairing: pairing(
                service
                    .cancel_pairing(
                        actor,
                        command_id,
                        operation_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsGatewayForget {
            gateway_ref,
            expected_revision,
        } => R::ConnectionsGateway {
            gateway: gateway(
                service
                    .forget_gateway(
                        actor,
                        command_id,
                        gateway_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsIntegrationPrepareReview {
            integration_ref,
            expected_revision,
        } => R::ConnectionsIntegrationReview {
            review: integration_review(
                service
                    .prepare_integration_review(
                        actor,
                        command_id,
                        integration_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsIntegrationStart {
            integration_ref,
            review_ref,
            expected_revision,
        } => R::ConnectionsOperation {
            operation: operation(
                service
                    .start_integration(
                        actor,
                        command_id,
                        integration_ref.get(),
                        review_in(review_ref).map_err(AppCommandFailure::NotAdmitted)?,
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsOperationCancel {
            operation_ref,
            expected_revision,
        } => R::ConnectionsOperation {
            operation: operation(
                service
                    .cancel_operation(
                        actor,
                        command_id,
                        operation_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsSourcePrepareReview {
            source_ref,
            expected_revision,
        } => R::ConnectionsSourceReview {
            review: source_review(
                service
                    .prepare_source_review(
                        actor,
                        command_id,
                        source_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsSourceConfigure {
            source_ref,
            review_ref,
            selected_resource_refs,
            expected_revision,
        } => R::ConnectionsSourceConfiguration {
            configuration: configuration(
                service
                    .configure_source(
                        actor,
                        command_id,
                        source_ref.get(),
                        review_in(review_ref).map_err(AppCommandFailure::NotAdmitted)?,
                        selected_resource_refs
                            .into_iter()
                            .map(|value| value.get())
                            .collect(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsDisconnect {
            source_ref,
            expected_revision,
        } => R::ConnectionsOperation {
            operation: operation(
                service
                    .disconnect(
                        actor,
                        command_id,
                        source_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsObservePrepareReview {
            source_ref,
            expected_revision,
            requested_processing,
        } => R::ConnectionsObserveReview {
            review: observe_review(
                service
                    .prepare_observe_review(
                        actor,
                        command_id,
                        source_ref.get(),
                        expected_revision,
                        processing_in(requested_processing),
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        C::ConnectionsObserveSet { mutation } => R::ConnectionsSource {
            source: source(match mutation {
                dto::ConnectionObserveSetMutationDto::Enable {
                    source_ref,
                    review_ref,
                    expected_revision,
                } => service
                    .apply_observe(
                        actor,
                        command_id,
                        source_ref.get(),
                        expected_revision,
                        review_in(review_ref).map_err(AppCommandFailure::NotAdmitted)?,
                        owner::ObserveDecision::Allow,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
                dto::ConnectionObserveSetMutationDto::Pause {
                    source_ref,
                    expected_revision,
                } => service
                    .pause_observe(
                        actor,
                        command_id,
                        source_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            })?,
        },
        C::ConnectionsGatewayManagementLaunch {
            gateway_ref,
            expected_revision,
        } => R::ConnectionsLaunch {
            launch_action: launch(
                service
                    .request_management_launch(
                        actor,
                        command_id,
                        gateway_ref.get(),
                        expected_revision,
                        scope,
                    )
                    .await
                    .map_err(command_failure)?,
            )?,
        },
        _ => return Err(AppCommandFailure::NotAdmitted(validation("command.kind"))),
    })
}

pub(crate) async fn query(
    owners: &floe_app::ReadyOwners,
    actor: &OwnerActor,
    query: dto::AppProductQueryDto,
    scope: &ExecutionScope,
) -> AppWireResult<dto::AppQueryResultDto> {
    use dto::{AppProductQueryDto as Q, AppQueryResultDto as R};
    let service = &owners.connections;
    Ok(match query {
        Q::ConnectionsOverview {} => R::ConnectionsOverview {
            overview: overview(
                service
                    .overview(actor, scope)
                    .await
                    .map_err(agent_failure)?,
            )?,
        },
        Q::ConnectionsPairingGet { operation_ref } => R::ConnectionsPairing {
            pairing: pairing(
                service
                    .get_pairing(actor, operation_ref.get(), scope)
                    .await
                    .map_err(agent_failure)?,
            )?,
        },
        Q::ConnectionsGatewayGet { gateway_ref } => R::ConnectionsGateway {
            gateway: gateway(
                service
                    .get_gateway(actor, gateway_ref.get(), scope)
                    .await
                    .map_err(agent_failure)?,
            )?,
        },
        Q::ConnectionsIntegrationInspectReview { review_ref } => R::ConnectionsIntegrationReview {
            review: integration_review(
                service
                    .inspect_integration_review(actor, review_in(review_ref)?, scope)
                    .await
                    .map_err(agent_failure)?,
            )?,
        },
        Q::ConnectionsOperationGet { operation_ref } => R::ConnectionsOperation {
            operation: operation(
                service
                    .get_operation(actor, operation_ref.get(), scope)
                    .await
                    .map_err(agent_failure)?,
            )?,
        },
        Q::ConnectionsSourceInspectReview { review_ref } => R::ConnectionsSourceReview {
            review: source_review(
                service
                    .inspect_source_review(actor, review_in(review_ref)?, scope)
                    .await
                    .map_err(agent_failure)?,
            )?,
        },
        Q::ConnectionsObserveInspectReview { review_ref } => R::ConnectionsObserveReview {
            review: observe_review(
                service
                    .inspect_observe_review(actor, review_in(review_ref)?, scope)
                    .await
                    .map_err(agent_failure)?,
            )?,
        },
        _ => return Err(validation("query.kind")),
    })
}

pub(crate) fn review_in(value: dto::ReviewRefDto) -> AppWireResult<floe_access::ReviewRef> {
    value.validate().map_err(validation)?;
    let mut digest = [0; 32];
    for (index, chunk) in value.digest.as_str().as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| validation("review_ref.digest"))?;
        digest[index] =
            u8::from_str_radix(text, 16).map_err(|_| validation("review_ref.digest"))?;
    }
    let reference = floe_access::ReviewRef {
        id: value.id.get(),
        revision: value.revision,
        digest,
    };
    reference.validate().map_err(agent_failure)?;
    Ok(reference)
}
fn review_out(value: floe_access::ReviewRef) -> AppWireResult<dto::ReviewRefDto> {
    value.validate().map_err(|_| internal_error())?;
    Ok(dto::ReviewRefDto {
        id: dto::UuidRefDto::new(value.id).ok_or_else(internal_error)?,
        revision: value.revision,
        digest: dto::DigestHex64Dto::new(
            value
                .digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
        .ok_or_else(internal_error)?,
    })
}
fn processing_in(value: dto::RequestedProcessingDto) -> owner::ProcessingChoice {
    match value {
        dto::RequestedProcessingDto::DeviceOnly => owner::ProcessingChoice::DeviceOnly,
        dto::RequestedProcessingDto::GatewayAllowed => owner::ProcessingChoice::GatewayAllowed,
    }
}

fn actions(values: Vec<owner::ConnectionAction>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| {
            match value {
                owner::ConnectionAction::Pair => "pair",
                owner::ConnectionAction::Forget => "forget",
                owner::ConnectionAction::Manage => "manage",
                owner::ConnectionAction::Cancel => "cancel",
                owner::ConnectionAction::Reobserve => "reobserve",
                owner::ConnectionAction::Configure => "configure",
                owner::ConnectionAction::Disconnect => "disconnect",
                owner::ConnectionAction::PrepareObserveReview => "prepare_observe_review",
                owner::ConnectionAction::PauseObserve => "pause_observe",
                owner::ConnectionAction::Allow => "allow",
                owner::ConnectionAction::Decline => "decline",
                owner::ConnectionAction::Start => "start",
            }
            .to_owned()
        })
        .collect()
}
macro_rules! action_conversion {
    ($function:ident, $dto:ident, [$($variant:ident),+]) => {
        fn $function(values: Vec<owner::ConnectionAction>) -> AppWireResult<Vec<dto::$dto>> {
            values.into_iter().map(|value| match value {
                $(owner::ConnectionAction::$variant => Ok(dto::$dto::$variant),)+
                _ => Err(internal_error()),
            }).collect()
        }
    };
}
action_conversion!(gateway_actions, GatewayActionDto, [Pair, Forget, Manage]);
action_conversion!(pairing_actions, PairingActionDto, [Cancel, Reobserve]);
action_conversion!(
    source_actions,
    SourceActionDto,
    [Configure, Disconnect, PrepareObserveReview, PauseObserve]
);
action_conversion!(source_review_actions, SourceReviewActionDto, [Configure]);
action_conversion!(
    observe_review_actions,
    ObserveReviewActionDto,
    [Allow, Decline]
);
action_conversion!(
    integration_review_actions,
    IntegrationReviewActionDto,
    [Start]
);
action_conversion!(
    operation_actions,
    ConnectionOperationActionDto,
    [Cancel, Reobserve]
);
fn failure(value: owner::ConnectionFailure) -> dto::ConnectionsFailureDto {
    dto::ConnectionsFailureDto {
        domain: match value.domain {
            owner::ConnectionFailureDomain::Connections => {
                dto::ConnectionsFailureDomainDto::Connections
            }
        },
        category: value.category,
        reason: match value.reason {
            owner::ConnectionFailureReason::IdentityChanged => "identity_changed",
            owner::ConnectionFailureReason::OperationUncertain => "operation_uncertain",
            owner::ConnectionFailureReason::StorageUnavailable => "storage_unavailable",
            owner::ConnectionFailureReason::Rejected => "rejected",
            owner::ConnectionFailureReason::Expired => "expired",
            owner::ConnectionFailureReason::Conflict => "conflict",
        }
        .to_owned(),
        incident_id: value.incident_id.to_string(),
        correlation_id: value.correlation_id.to_string(),
        reload_required: value.reload_required,
        seal_session: value.seal_session,
        recovery: match value.recovery {
            owner::ConnectionRecovery::None => dto::ConnectionsRecoveryDto::None,
            owner::ConnectionRecovery::Reobserve => dto::ConnectionsRecoveryDto::Reobserve,
            owner::ConnectionRecovery::Reconcile => dto::ConnectionsRecoveryDto::Reconcile,
            owner::ConnectionRecovery::Unlock => dto::ConnectionsRecoveryDto::Unlock,
            owner::ConnectionRecovery::Reopen => dto::ConnectionsRecoveryDto::Reopen,
            owner::ConnectionRecovery::NewReview => dto::ConnectionsRecoveryDto::NewReview,
        },
        safe_actions: actions(value.safe_actions),
    }
}
fn gateway(value: owner::GatewaySummary) -> AppWireResult<dto::GatewaySummaryDto> {
    Ok(dto::GatewaySummaryDto {
        display_address: value.display_address,
        gateway_ref: dto::GatewayRefDto::new(value.gateway_ref).ok_or_else(internal_error)?,
        revision: value.revision,
        display_name: value.display_name,
        state: match value.state {
            owner::GatewayState::Unpaired => dto::GatewayStateDto::Unpaired,
            owner::GatewayState::Paired => dto::GatewayStateDto::Paired,
            owner::GatewayState::RepairRequired => dto::GatewayStateDto::RepairRequired,
            owner::GatewayState::Forgotten => dto::GatewayStateDto::Forgotten,
        },
        allowed_actions: gateway_actions(value.allowed_actions)?,
        failure: value.failure.map(failure),
        remote_revocation_pending: value.remote_revocation_pending,
    })
}
fn pairing(value: owner::PairingSnapshot) -> AppWireResult<dto::PairingSnapshotDto> {
    Ok(dto::PairingSnapshotDto {
        operation_ref: dto::OperationRefDto::new(value.operation_ref).ok_or_else(internal_error)?,
        revision: value.revision,
        state: match value.state {
            owner::PairingState::Pending => dto::PairingStateDto::Starting,
            owner::PairingState::AwaitingLocalConfirmation => dto::PairingStateDto::Starting,
            owner::PairingState::AwaitingApproval => dto::PairingStateDto::AwaitingGatewayApproval,
            owner::PairingState::Cancelling => dto::PairingStateDto::Cancelling,
            owner::PairingState::Paired => dto::PairingStateDto::Connected,
            owner::PairingState::Rejected => dto::PairingStateDto::Rejected,
            owner::PairingState::Expired => dto::PairingStateDto::Expired,
            owner::PairingState::Cancelled => dto::PairingStateDto::Cancelled,
            owner::PairingState::RepairRequired => dto::PairingStateDto::RepairRequired,
            owner::PairingState::RevocationPending => dto::PairingStateDto::RevocationPending,
            owner::PairingState::Forgotten => dto::PairingStateDto::Forgotten,
        },
        display_code: value.display_code,
        expires_at: value.expires_at.map(|time| time.to_rfc3339()),
        gateway: value.gateway.map(gateway).transpose()?,
        allowed_actions: pairing_actions(value.allowed_actions)?,
        failure: value.failure.map(failure),
        next_observation_after_ms: value.next_observation_after_ms.map(u64::from),
    })
}
fn resource_group(value: owner::ResourceGroupSummary) -> AppWireResult<dto::ResourceGroupDto> {
    Ok(dto::ResourceGroupDto {
        group_ref: dto::ResourceGroupRefDto::new(value.group_ref).ok_or_else(internal_error)?,
        label: value.label,
    })
}
fn configuration(
    value: owner::SourceConfigurationResult,
) -> AppWireResult<dto::SourceConfigurationResultDto> {
    let result = match value {
        owner::SourceConfigurationResult::Configured { source: value } => {
            dto::SourceConfigurationResultDto::Configured {
                source: source(value)?,
            }
        }
        owner::SourceConfigurationResult::NotSavedReviewRequired { source_ref } => {
            dto::SourceConfigurationResultDto::NotSavedReviewRequired {
                source_ref: dto::ConnectionsSourceRefDto::new(source_ref)
                    .ok_or_else(internal_error)?,
            }
        }
    };
    result.validate().map_err(|_| internal_error())?;
    Ok(result)
}

fn source(value: owner::SourceSummary) -> AppWireResult<dto::SourceSummaryDto> {
    Ok(dto::SourceSummaryDto {
        source_ref: dto::ConnectionsSourceRefDto::new(value.source_ref)
            .ok_or_else(internal_error)?,
        revision: value.revision,
        display_labels: value.display_labels,
        availability: match value.availability {
            owner::SourceAvailability::Available => dto::SourceAvailabilityDto::Available,
            owner::SourceAvailability::Unavailable => dto::SourceAvailabilityDto::Unavailable,
            owner::SourceAvailability::PermissionRequired => {
                dto::SourceAvailabilityDto::PermissionRequired
            }
            owner::SourceAvailability::IdentityChanged => {
                dto::SourceAvailabilityDto::IdentityChanged
            }
            owner::SourceAvailability::Disconnected => dto::SourceAvailabilityDto::Disconnected,
        },
        last_observed_at: value.last_observed_at.map(|time| time.to_rfc3339()),
        selected_resources: value
            .selected_resources
            .into_iter()
            .map(|resource| {
                Ok(dto::SelectedResourceDto {
                    resource_ref: dto::ResourceRefDto::new(resource.resource_ref)
                        .ok_or_else(internal_error)?,
                    label: resource.label,
                    group: resource.group.map(resource_group).transpose()?,
                })
            })
            .collect::<AppWireResult<_>>()?,
        observe_state: match value.observe_state {
            owner::ObserveState::Disabled => dto::ObserveStateDto::Disabled,
            owner::ObserveState::Enabled => dto::ObserveStateDto::Enabled,
            owner::ObserveState::Paused => dto::ObserveStateDto::Paused,
            owner::ObserveState::ReviewRequired => dto::ObserveStateDto::ReviewRequired,
        },
        allowed_actions: source_actions(value.allowed_actions)?,
    })
}
fn overview(value: owner::ConnectionsOverview) -> AppWireResult<dto::ConnectionsOverviewDto> {
    Ok(dto::ConnectionsOverviewDto {
        gateways: value
            .gateways
            .into_iter()
            .map(gateway)
            .collect::<AppWireResult<_>>()?,
        integrations: value
            .integrations
            .into_iter()
            .map(integration)
            .collect::<AppWireResult<_>>()?,
        sources: value
            .sources
            .into_iter()
            .map(source)
            .collect::<AppWireResult<_>>()?,
    })
}
fn integration(value: owner::IntegrationSummary) -> AppWireResult<dto::IntegrationSummaryDto> {
    Ok(dto::IntegrationSummaryDto {
        service_kind: match value.service_kind {
            owner::IntegrationServiceKind::AppleCalendar => {
                dto::IntegrationServiceKindDto::AppleCalendar
            }
            #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
            owner::IntegrationServiceKind::SyntheticQaCalendar => {
                dto::IntegrationServiceKindDto::SyntheticQaCalendar
            }
            owner::IntegrationServiceKind::AppleContacts => {
                dto::IntegrationServiceKindDto::AppleContacts
            }
            owner::IntegrationServiceKind::AppleHealth => {
                dto::IntegrationServiceKindDto::AppleHealth
            }
            owner::IntegrationServiceKind::AppleAttention => {
                dto::IntegrationServiceKindDto::AppleAttention
            }
            owner::IntegrationServiceKind::Hosted => dto::IntegrationServiceKindDto::Hosted,
        },
        integration_ref: dto::IntegrationRefDto::new(value.integration_ref)
            .ok_or_else(internal_error)?,
        revision: value.revision,
        display_name: value.display_name,
        category: match value.category.as_str() {
            "calendar" => dto::IntegrationCategoryDto::Calendar,
            "contacts" => dto::IntegrationCategoryDto::Contacts,
            "attention" => dto::IntegrationCategoryDto::Attention,
            "health" => dto::IntegrationCategoryDto::Health,
            "mail" => dto::IntegrationCategoryDto::Mail,
            "work" => dto::IntegrationCategoryDto::Work,
            "home" => dto::IntegrationCategoryDto::Home,
            _ => return Err(internal_error()),
        },
        state: match value.state {
            owner::IntegrationState::Available => dto::IntegrationStateDto::Available,
            owner::IntegrationState::Unavailable => dto::IntegrationStateDto::Unavailable,
            owner::IntegrationState::Connected => dto::IntegrationStateDto::Connected,
            owner::IntegrationState::Connecting => dto::IntegrationStateDto::Connecting,
            owner::IntegrationState::Error => dto::IntegrationStateDto::Error,
        },
        capabilities: value
            .capabilities
            .into_iter()
            .map(|value| match value {
                owner::IntegrationCapability::PrepareReview => {
                    dto::IntegrationCapabilityDto::PrepareReview
                }
                owner::IntegrationCapability::Configure => dto::IntegrationCapabilityDto::Configure,
                owner::IntegrationCapability::Disconnect => {
                    dto::IntegrationCapabilityDto::Disconnect
                }
            })
            .collect(),
        source: value.source.map(source).transpose()?,
    })
}

#[cfg(all(test, feature = "qa-fixtures", target_os = "linux"))]
mod tests {
    use super::{dto, integration, owner};
    use uuid::Uuid;

    #[test]
    fn synthetic_calendar_integration_maps_to_its_own_wire_kind() {
        let value = owner::IntegrationSummary {
            service_kind: owner::IntegrationServiceKind::SyntheticQaCalendar,
            integration_ref: Uuid::new_v4(),
            revision: 1,
            display_name: "Synthetic QA Calendar".into(),
            category: "calendar".into(),
            state: owner::IntegrationState::Available,
            capabilities: vec![],
            source: None,
        };

        let wire = integration(value).expect("map synthetic integration");
        assert_eq!(
            wire.service_kind,
            dto::IntegrationServiceKindDto::SyntheticQaCalendar
        );
        let json = serde_json::to_value(wire).expect("serialize synthetic integration");
        assert_eq!(json["service_kind"], "synthetic_qa_calendar");
    }
}
fn operation(
    value: owner::ConnectionOperationSnapshot,
) -> AppWireResult<dto::ConnectionOperationSnapshotDto> {
    Ok(dto::ConnectionOperationSnapshotDto {
        operation_ref: dto::OperationRefDto::new(value.operation_ref).ok_or_else(internal_error)?,
        revision: value.revision,
        state: match value.state {
            owner::ConnectionOperationState::Pending => dto::ConnectionOperationStateDto::Pending,
            owner::ConnectionOperationState::Running => dto::ConnectionOperationStateDto::Running,
            owner::ConnectionOperationState::AwaitingUser => {
                dto::ConnectionOperationStateDto::AwaitingUser
            }
            owner::ConnectionOperationState::Completed => {
                dto::ConnectionOperationStateDto::Completed
            }
            owner::ConnectionOperationState::Failed => dto::ConnectionOperationStateDto::Failed,
            owner::ConnectionOperationState::Cancelled => {
                dto::ConnectionOperationStateDto::Cancelled
            }
            owner::ConnectionOperationState::RepairRequired => {
                dto::ConnectionOperationStateDto::RepairRequired
            }
        },
        launch_action: value.launch_action.map(launch).transpose()?,
        display_code: value.display_code,
        source: value.source.map(source).transpose()?,
        allowed_actions: operation_actions(value.allowed_actions)?,
        failure: value.failure.map(failure),
        next_observation_after_ms: value.next_observation_after_ms.map(u64::from),
    })
}
fn processing_scope(
    value: floe_context_contract::ProcessingRestriction,
) -> dto::ProcessingScopeDto {
    match value {
        floe_context_contract::ProcessingRestriction::DeviceOnly => {
            dto::ProcessingScopeDto::DeviceOnly
        }
        floe_context_contract::ProcessingRestriction::GatewayAllowed { categories } => {
            dto::ProcessingScopeDto::GatewayAllowed {
                categories: categories.into_iter().map(data_category).collect(),
            }
        }
    }
}
fn data_category(value: floe_context_contract::GrantDataCategory) -> dto::ProcessingCategoryDto {
    match value {
        floe_context_contract::GrantDataCategory::Metadata => dto::ProcessingCategoryDto::Metadata,
        floe_context_contract::GrantDataCategory::Content => dto::ProcessingCategoryDto::Content,
        floe_context_contract::GrantDataCategory::Derived => dto::ProcessingCategoryDto::Derived,
    }
}
fn view_sensitivity(
    value: floe_context_contract::DataClass,
) -> AppWireResult<dto::ViewSensitivityDto> {
    match value {
        floe_context_contract::DataClass::Personal => Ok(dto::ViewSensitivityDto::Personal),
        floe_context_contract::DataClass::HighlySensitive => {
            Ok(dto::ViewSensitivityDto::HighlySensitive)
        }
        _ => Err(internal_error()),
    }
}
fn disclosure(value: owner::ProcessingDisclosure) -> AppWireResult<dto::ProcessingDisclosureDto> {
    Ok(dto::ProcessingDisclosureDto {
        views: value
            .views
            .into_iter()
            .map(|view| {
                Ok(dto::ViewProcessingDisclosureDto {
                    view_id: view.view_id,
                    data_class: view_sensitivity(view.data_class)?,
                    data_categories: view
                        .data_categories
                        .into_iter()
                        .map(data_category)
                        .collect(),
                    current: view.current.map(processing_scope),
                    requested: view.requested.map(processing_scope),
                })
            })
            .collect::<AppWireResult<_>>()?,
    })
}
fn source_review(value: owner::SourceReview) -> AppWireResult<dto::SourceReviewDto> {
    Ok(dto::SourceReviewDto {
        review_ref: review_out(value.review_ref)?,
        source_ref: dto::ConnectionsSourceRefDto::new(value.source_ref)
            .ok_or_else(internal_error)?,
        source_revision: value.source_revision,
        labels: value.labels,
        permitted_choices: value
            .permitted_choices
            .into_iter()
            .map(|resource| {
                Ok(dto::PermittedResourceChoiceDto {
                    resource_ref: dto::ResourceRefDto::new(resource.resource_ref)
                        .ok_or_else(internal_error)?,
                    label: resource.label,
                    group: resource.group.map(resource_group).transpose()?,
                    selected: resource.selected,
                })
            })
            .collect::<AppWireResult<_>>()?,
        processing_disclosure: disclosure(value.processing_disclosure)?,
        expires_at: value.expires_at.to_rfc3339(),
        allowed_actions: source_review_actions(value.allowed_actions)?,
    })
}
pub(crate) fn observe_review(value: owner::ObserveReview) -> AppWireResult<dto::ObserveReviewDto> {
    Ok(dto::ObserveReviewDto {
        review_ref: review_out(value.review_ref)?,
        source_ref: dto::ConnectionsSourceRefDto::new(value.source_ref)
            .ok_or_else(internal_error)?,
        source_revision: value.source_revision,
        display_members: value.display_members,
        processing_disclosure: disclosure(value.processing_disclosure)?,
        expires_at: value.expires_at.to_rfc3339(),
        allowed_actions: observe_review_actions(value.allowed_actions)?,
    })
}
fn integration_review(value: owner::IntegrationReview) -> AppWireResult<dto::IntegrationReviewDto> {
    Ok(dto::IntegrationReviewDto {
        review_ref: review_out(value.review_ref)?,
        integration_ref: dto::IntegrationRefDto::new(value.integration_ref)
            .ok_or_else(internal_error)?,
        catalog_revision: value.catalog_revision,
        target: match value.target {
            owner::IntegrationTarget::Device { device_id } => {
                dto::IntegrationTargetDto::Device { device_id }
            }
            owner::IntegrationTarget::Gateway {
                gateway_ref,
                gateway_revision,
            } => dto::IntegrationTargetDto::Gateway {
                gateway_ref: dto::GatewayRefDto::new(gateway_ref).ok_or_else(internal_error)?,
                gateway_revision,
            },
        },
        display_name: value.display_name,
        setup_kind: match value.setup_kind {
            owner::IntegrationSetupKind::BrowserAuthorization => {
                dto::IntegrationSetupKindDto::BrowserAuthorization
            }
            owner::IntegrationSetupKind::DeviceCode => dto::IntegrationSetupKindDto::DeviceCode,
            owner::IntegrationSetupKind::GatewayManagedSecret => {
                dto::IntegrationSetupKindDto::GatewayManagedSecret
            }
            owner::IntegrationSetupKind::NativePermission => {
                dto::IntegrationSetupKindDto::NativePermission
            }
        },
        expires_at: value.expires_at.to_rfc3339(),
        allowed_actions: integration_review_actions(value.allowed_actions)?,
    })
}
fn launch(value: owner::ValidatedManagementLaunch) -> AppWireResult<dto::LaunchActionDto> {
    Ok(dto::LaunchActionDto {
        action_ref: dto::LaunchActionRefDto::new(value.action_ref).ok_or_else(internal_error)?,
        purpose: match value.purpose {
            owner::LaunchPurpose::ManageGateway => dto::LaunchPurposeDto::ManageGateway,
            owner::LaunchPurpose::AuthorizeIntegration => {
                dto::LaunchPurposeDto::AuthorizeIntegration
            }
        },
        validated_url: value.validated_url,
        expires_at: value.expires_at.to_rfc3339(),
    })
}
