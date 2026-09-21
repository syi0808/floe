use crate::app_wire::{
    AppWireResult, host_failure, internal_error, request_validation, service_error,
};
use crate::bridge::FloeHandle;
use crate::conversion::worker::*;
use floe_protocol::*;

pub(crate) fn pairing(
    handle: &FloeHandle,
    request: RemotePairingRequestDto,
) -> AppWireResult<RemotePairingResultDto> {
    pairing_with_host(&handle.app(), request)
}

fn pairing_with_host<Services: floe_app::HostServices + floe_app::RemotePairingCommands>(
    host: &floe_app::AppHost<Services>,
    request: RemotePairingRequestDto,
) -> AppWireResult<RemotePairingResultDto> {
    request.validate().map_err(request_validation)?;
    let admitted = host.request(request.request_id).map_err(host_failure)?;
    let service = admitted.services();
    let caller = admitted.caller();
    if let RemotePairingOperationDto::ReadResult {
        operation_id,
        release,
    } = request.operation
    {
        return pairing_result(
            service
                .read_pairing_result(caller, operation_id, release)
                .map_err(service_error)?,
        );
    }
    let command = match request.operation {
        RemotePairingOperationDto::Prepare {} => floe_app::RemotePairingCommand::Prepare,
        RemotePairingOperationDto::Confirm {
            target,
            challenge,
            polling_proof,
        } => {
            let issuer_fingerprint = challenge.issuer.fingerprint.clone();
            floe_app::RemotePairingCommand::Confirm {
                target: floe_app::PairingTarget {
                    base_url: target.base_url,
                },
                challenge: Box::new(pairing_challenge(challenge)),
                issuer_fingerprint,
                polling_proof,
            }
        }
        RemotePairingOperationDto::Status {
            target,
            pairing_id,
            polling_proof,
        } => floe_app::RemotePairingCommand::Status {
            target: floe_app::PairingTarget {
                base_url: target.base_url,
            },
            pairing_id: pairing_id.to_string(),
            polling_proof,
        },
        RemotePairingOperationDto::Finalize {
            target,
            pairing_id,
            polling_proof,
            challenge,
        } => {
            let issuer_fingerprint = challenge.issuer.fingerprint.clone();
            floe_app::RemotePairingCommand::Finalize {
                target: floe_app::PairingTarget {
                    base_url: target.base_url,
                },
                pairing_id: pairing_id.to_string(),
                polling_proof,
                challenge: Box::new(pairing_challenge(challenge)),
                issuer_fingerprint,
            }
        }
        RemotePairingOperationDto::ReadResult { .. } => unreachable!(),
    };
    pairing_result(
        service
            .remote_pairing(caller, request.request_id, command)
            .map_err(service_error)?,
    )
}

fn pairing_challenge(challenge: RemotePairingChallengeDto) -> floe_app::RemotePairingChallenge {
    floe_app::RemotePairingChallenge {
        pairing_id: challenge.pairing_id,
        challenge_id: challenge.challenge_id,
        challenge_b64url: challenge.challenge_b64url,
        producer_signature: challenge.producer_signature,
        producer: producer_identity(&challenge.producer),
        issuer: floe_app::RemoteOwnerPublicKey {
            key_id: challenge.issuer.key_id,
            public_key: challenge.issuer.public_key,
        },
        expires_at_unix_ms: challenge.expires_at_unix_ms,
    }
}

fn pairing_report(status: floe_app::PairingStatus) -> AppWireResult<PairingReportDto> {
    let outcome = match (status.status.as_str(), status.client_id, status.token) {
        ("pending", None, None) => PairingOutcomeDto::Pending {},
        ("local_confirmed", None, None) => PairingOutcomeDto::LocalConfirmed {},
        ("approved", Some(client_id), Some(token)) => {
            PairingOutcomeDto::Approved { client_id, token }
        }
        ("rejected", None, None) => PairingOutcomeDto::Rejected {},
        ("expired", None, None) => PairingOutcomeDto::Expired {},
        ("repair_required", None, None) => PairingOutcomeDto::RepairRequired {},
        _ => return Err(internal_error()),
    };
    Ok(PairingReportDto {
        pairing_id: status.pairing_id,
        person_id: status.person_id,
        device_id: status.device_id,
        producer: status.producer.map(|producer| RemoteProducerIdentityDto {
            schema_version: producer.schema_version,
            instance_id: producer.instance_id,
            execution_owner: producer.execution_owner,
            audience: producer.audience,
            key_id: producer.key_id,
            public_key: producer.public_key,
            fingerprint: producer.fingerprint,
        }),
        issuer: status.issuer.as_ref().map(pairing_issuer_dto),
        issuer_fingerprint: status.issuer_fingerprint,
        outcome,
    })
}

fn pairing_result(result: floe_app::RemotePairingResult) -> AppWireResult<RemotePairingResultDto> {
    Ok(RemotePairingResultDto {
        operation_id: result.operation_id,
        done: result.done,
        owner: result.owner.as_ref().map(owner_key_dto),
        pairing: result.pairing.map(pairing_report).transpose()?,
        failure: result.failure.as_ref().map(|failure| {
            failure_envelope(failure, &result.stage, &result.operation_id.to_string())
        }),
    })
}

pub(crate) fn access(
    handle: &FloeHandle,
    request: RemoteAccessRequestDto,
) -> AppWireResult<RemoteAccessResultDto> {
    access_with_host(&handle.app(), request)
}

fn access_with_host<Services: floe_app::HostServices + floe_app::RemoteAccessCommands>(
    host: &floe_app::AppHost<Services>,
    request: RemoteAccessRequestDto,
) -> AppWireResult<RemoteAccessResultDto> {
    request.validate().map_err(request_validation)?;
    let admitted = host.request(request.request_id).map_err(host_failure)?;
    let service = admitted.services();
    let caller = admitted.caller();
    if let RemoteAccessOperationDto::ReadResult {
        operation_id,
        release,
    } = request.operation
    {
        return access_result(
            service
                .read_access_result(caller, operation_id, release)
                .map_err(service_error)?,
        );
    }
    let command = match request.operation {
        RemoteAccessOperationDto::InspectProducer {} => {
            floe_app::RemoteAccessCommand::InspectProducer
        }
        RemoteAccessOperationDto::ReviewAndEnroll { producer } => {
            floe_app::RemoteAccessCommand::ReviewAndEnroll {
                producer: Box::new(producer_identity(&producer)),
            }
        }
        RemoteAccessOperationDto::EnrollmentStatus { enrollment_id } => {
            floe_app::RemoteAccessCommand::EnrollmentStatus { enrollment_id }
        }
        RemoteAccessOperationDto::CalendarGrantPreview {
            connector_id,
            connection_id,
            resource,
        } => floe_app::RemoteAccessCommand::CalendarGrantPreview {
            connector_id,
            connection_id,
            resource,
        },
        RemoteAccessOperationDto::CalendarGrantReview {
            connector_id,
            connection_id,
            resource,
            expected_producer_fingerprint,
        } => floe_app::RemoteAccessCommand::CalendarGrantReview {
            connector_id,
            connection_id,
            resource,
            expected_producer_fingerprint,
        },
        RemoteAccessOperationDto::CalendarGrantStatus { grant_id } => {
            floe_app::RemoteAccessCommand::CalendarGrantStatus { grant_id }
        }
        RemoteAccessOperationDto::CalendarGrantPause {
            grant_id,
            expected_authority,
        } => floe_app::RemoteAccessCommand::CalendarGrantPause {
            grant_id,
            expected_authority,
        },
        RemoteAccessOperationDto::ViewGrantPreview {
            view_id,
            connector_id,
            connection_id,
            resource,
            consumer,
        } => floe_app::RemoteAccessCommand::ViewGrantPreview {
            view_id,
            connector_id,
            connection_id,
            resource,
            consumer,
        },
        RemoteAccessOperationDto::ViewGrantReview {
            view_id,
            connector_id,
            connection_id,
            resource,
            consumer,
            expected_producer_fingerprint,
            expected_source_authority,
            expected_connection_revision,
            expected_provider_identity,
            expected_recipient,
        } => floe_app::RemoteAccessCommand::ViewGrantReview {
            view_id,
            connector_id,
            connection_id,
            resource,
            consumer,
            expected_producer_fingerprint,
            expected_source_authority,
            expected_connection_revision,
            expected_provider_identity,
            expected_recipient,
        },
        RemoteAccessOperationDto::ViewGrantStatus { grant_id } => {
            floe_app::RemoteAccessCommand::ViewGrantStatus { grant_id }
        }
        RemoteAccessOperationDto::ViewGrantPause {
            grant_id,
            expected_authority,
        } => floe_app::RemoteAccessCommand::ViewGrantPause {
            grant_id,
            expected_authority,
        },
        RemoteAccessOperationDto::ReadResult { .. } => unreachable!(),
    };
    access_result(
        service
            .remote_access(caller, request.request_id, command)
            .map_err(service_error)?,
    )
}

fn access_result(result: floe_app::RemoteAccessResult) -> AppWireResult<RemoteAccessResultDto> {
    Ok(RemoteAccessResultDto {
        operation_id: result.operation_id,
        done: result.done,
        producer: result.producer.as_ref().map(producer_identity_dto),
        owner: result.owner.as_ref().map(owner_key_dto),
        enrollment: result.enrollment.map(enrollment_status_dto),
        calendar_grant: result
            .calendar_grant
            .as_ref()
            .map(|overview| remote_calendar_grant_overview(&overview.grant))
            .transpose()
            .map_err(|_| internal_error())?,
        calendar_preview: result
            .calendar_preview
            .map(|preview| remote_calendar_preview_dto(result.person_id, preview)),
        view_grant: result
            .view_grant
            .as_ref()
            .map(|overview| {
                remote_view_grant_overview(&overview.grant, overview.connection_revision)
            })
            .transpose()
            .map_err(|_| internal_error())?,
        view_preview: result
            .view_preview
            .as_ref()
            .map(|preview| remote_view_grant_preview(result.person_id, preview)),
        failure: result.failure.as_ref().map(|failure| {
            failure_envelope(failure, &result.stage, &result.operation_id.to_string())
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    #[derive(Clone, Default)]
    struct Services(Arc<Mutex<Vec<(floe_app::CallerContext, Uuid, String)>>>);

    impl floe_app::HostServices for Services {
        fn shutdown(&self) -> Result<(), floe_app::HostError> {
            Ok(())
        }
    }

    impl floe_app::RemotePairingCommands for Services {
        fn remote_pairing(
            &self,
            caller: &floe_app::CallerContext,
            request_id: Uuid,
            command: floe_app::RemotePairingCommand,
        ) -> Result<floe_app::RemotePairingResult, floe_app::ServiceError> {
            self.0
                .lock()
                .unwrap()
                .push((caller.clone(), request_id, format!("{command:?}")));
            Ok(floe_app::RemotePairingResult {
                operation_id: request_id,
                stage: "remote_pairing_status".into(),
                done: true,
                owner: None,
                pairing: None,
                failure: None,
            })
        }
        fn read_pairing_result(
            &self,
            caller: &floe_app::CallerContext,
            operation_id: Uuid,
            release: bool,
        ) -> Result<floe_app::RemotePairingResult, floe_app::ServiceError> {
            self.0
                .lock()
                .unwrap()
                .push((caller.clone(), operation_id, format!("read:{release}")));
            Ok(floe_app::RemotePairingResult {
                operation_id,
                stage: "remote_pairing_status".into(),
                done: false,
                owner: None,
                pairing: None,
                failure: None,
            })
        }
    }

    impl floe_app::RemoteAccessCommands for Services {
        fn remote_access(
            &self,
            caller: &floe_app::CallerContext,
            request_id: Uuid,
            command: floe_app::RemoteAccessCommand,
        ) -> Result<floe_app::RemoteAccessResult, floe_app::ServiceError> {
            self.0
                .lock()
                .unwrap()
                .push((caller.clone(), request_id, format!("{command:?}")));
            Err(floe_app::ServiceError::AccessDenied)
        }
        fn read_access_result(
            &self,
            caller: &floe_app::CallerContext,
            operation_id: Uuid,
            release: bool,
        ) -> Result<floe_app::RemoteAccessResult, floe_app::ServiceError> {
            self.0
                .lock()
                .unwrap()
                .push((caller.clone(), operation_id, format!("read:{release}")));
            Err(floe_app::ServiceError::NotFound)
        }
    }

    #[test]
    fn remote_handlers_use_verified_host_identity_and_owner_services() {
        let services = Services::default();
        let person_id = Uuid::new_v4();
        let host = floe_app::AppHost::bootstrap_claim(
            services.clone(),
            floe_app::LocalIdentityClaim {
                person_id,
                device_id: "verified-mac".into(),
            },
        )
        .unwrap();
        let request_id = Uuid::new_v4();
        pairing_with_host(
            &host,
            RemotePairingRequestDto {
                schema_version: 2,
                request_id,
                operation: RemotePairingOperationDto::Status {
                    target: PairingTargetDto {
                        base_url: "http://localhost:8431".into(),
                    },
                    pairing_id: Uuid::new_v4(),
                    polling_proof: "private_polling_proof".into(),
                },
            },
        )
        .unwrap();
        let denied = access_with_host(
            &host,
            RemoteAccessRequestDto {
                schema_version: 2,
                request_id,
                operation: RemoteAccessOperationDto::InspectProducer {},
            },
        )
        .unwrap_err();
        assert_eq!(denied.code, AppWireErrorCodeDto::AccessDenied);
        pairing_with_host(
            &host,
            RemotePairingRequestDto {
                schema_version: 2,
                request_id: Uuid::new_v4(),
                operation: RemotePairingOperationDto::ReadResult {
                    operation_id: request_id,
                    release: true,
                },
            },
        )
        .unwrap();
        let captured = services.0.lock().unwrap();
        assert_eq!(captured.len(), 3);
        for (caller, operation_id, debug) in captured.iter() {
            assert_eq!(caller.person_id(), person_id);
            assert_eq!(caller.device_id(), "verified-mac");
            assert!(caller.runtime_epoch() > 0);
            assert_eq!(*operation_id, request_id);
            assert!(!debug.contains("private_polling_proof"));
        }
        drop(captured);
        host.shutdown().unwrap();
        assert_eq!(
            access_with_host(
                &host,
                RemoteAccessRequestDto {
                    schema_version: 2,
                    request_id,
                    operation: RemoteAccessOperationDto::InspectProducer {}
                }
            )
            .unwrap_err()
            .code,
            AppWireErrorCodeDto::Unavailable
        );
        assert_eq!(services.0.lock().unwrap().len(), 3);
    }

    #[derive(Clone)]
    struct TraceBuffer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for TraceBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn approved_token_is_serializable_but_omitted_from_debug_and_conversion_errors() {
        let token = "new_pairing_secret_for_secure_persistence";
        let status = floe_app::PairingStatus {
            schema_version: 1,
            pairing_id: Uuid::new_v4().to_string(),
            status: "approved".into(),
            person_id: Uuid::new_v4().to_string(),
            device_id: "verified-mac".into(),
            producer: None,
            issuer: None,
            issuer_fingerprint: None,
            client_id: Some("client".into()),
            token: Some(token.into()),
        };
        assert!(!format!("{status:?}").contains(token));
        let report = pairing_report(status.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&report).unwrap()["outcome"]["token"],
            token
        );
        assert!(!format!("{report:?}").contains(token));
        let trace = TraceBuffer(Arc::new(Mutex::new(Vec::new())));
        let output = trace.0.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || trace.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(report = ?report, failure = ?internal_error(), "pairing_result");
        });
        let trace = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(trace.contains("pairing_result"));
        assert!(!trace.contains(token));
        let result = floe_app::RemotePairingResult {
            operation_id: Uuid::new_v4(),
            stage: "remote_pairing_finalize".into(),
            done: true,
            owner: None,
            pairing: Some(status.clone()),
            failure: None,
        };
        assert!(!format!("{result:?}").contains(token));
        assert!(!format!("{:?}", pairing_result(result).unwrap()).contains(token));
        let error = pairing_report(floe_app::PairingStatus {
            status: "pending".into(),
            ..status
        })
        .unwrap_err();
        assert!(!format!("{error:?}").contains(token));
        assert!(!serde_json::to_string(&error).unwrap().contains(token));
        assert!(
            !format!(
                "{:?}",
                failure_envelope(
                    &floe_app::AgentFailure::PolicyDenied,
                    "remote_pairing",
                    "operation"
                )
            )
            .contains(token)
        );
    }
}
