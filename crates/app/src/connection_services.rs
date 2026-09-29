use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::personal_source_spec::PersonalSourceSpec;
use crate::{
    AgentFailure, AppComposition, CallerContext, CoreError, ErrorCode, ServiceError, VaultState,
};
use floe_access::{PersonalSubjectInspector, PersonalSubjectProbe, valid_subject_fingerprint};
use floe_connections::{
    ConnectionId, ConnectionResource, ConnectorId, ResourceMode, SourceConnection,
    SourceConnectionError, SourceRepositoryError, SourceServiceError,
};
use floe_context::{NativeCalendarSubjectSource, NativeSubjectRequest};
use floe_context_contract::{CalendarProvider, ExecutionOwnerId};
use floe_kernel::PersonId;
use uuid::Uuid;

const EVENT_KIT_CONNECTOR: &str = "calendar.event_kit";
const REMOTE_CALENDAR_CONNECTORS: [&str; 2] = ["calendar.google", "calendar.microsoft"];

pub enum RemoteCalendarSourceMutation {
    Bind {
        connector_id: ConnectorId,
        connection_id: ConnectionId,
        expected_revision: Option<u64>,
        resources: Vec<ConnectionResource>,
    },
    Disconnect {
        connection_id: ConnectionId,
        expected_revision: u64,
    },
}

pub trait RemoteCalendarSourceCommands {
    fn inspect_remote_calendar_sources(
        &self,
        caller: &CallerContext,
    ) -> Result<Vec<SourceConnection>, CoreError>;

    fn mutate_remote_calendar_source(
        &self,
        caller: &CallerContext,
        mutation: RemoteCalendarSourceMutation,
    ) -> Result<SourceConnection, CoreError>;
}

impl RemoteCalendarSourceCommands for AppComposition {
    fn inspect_remote_calendar_sources(
        &self,
        caller: &CallerContext,
    ) -> Result<Vec<SourceConnection>, CoreError> {
        let person_id = PersonId(caller.person_id());
        self.runtime.block_on(async {
            let service = self.core.source_service();
            let mut sources = Vec::new();
            for connector in REMOTE_CALENDAR_CONNECTORS {
                sources.extend(
                    service
                        .list_current(
                            person_id,
                            &ConnectorId::try_new(connector).expect("constant connector ID"),
                        )
                        .await
                        .map_err(source_error)?
                        .into_iter()
                        .filter(|source| {
                            source.execution_owner_id().as_str() == caller.device_id()
                        }),
                );
            }
            if sources.len() > 1 {
                return Err(CoreError::new(
                    ErrorCode::Conflict,
                    "multiple current remote Calendar sources",
                ));
            }
            Ok(sources)
        })
    }

    fn mutate_remote_calendar_source(
        &self,
        caller: &CallerContext,
        mutation: RemoteCalendarSourceMutation,
    ) -> Result<SourceConnection, CoreError> {
        let person_id = PersonId(caller.person_id());
        self.runtime.block_on(async {
            let service = self.core.source_service();
            match mutation {
                RemoteCalendarSourceMutation::Bind {
                    connector_id,
                    connection_id,
                    expected_revision,
                    resources,
                } => {
                    if !REMOTE_CALENDAR_CONNECTORS.contains(&connector_id.as_str()) {
                        return Err(CoreError::new(
                            ErrorCode::Validation,
                            "invalid remote Calendar connector",
                        ));
                    }
                    if let Some(expected_revision) = expected_revision {
                        let existing = verify_remote_source(
                            &service,
                            person_id,
                            caller.device_id(),
                            &connection_id,
                        )
                        .await?;
                        if existing.connector_id() != &connector_id {
                            return Err(CoreError::new(
                                ErrorCode::Conflict,
                                "remote Calendar connector changed",
                            ));
                        }
                        service
                            .configure(
                                person_id,
                                &connection_id,
                                expected_revision,
                                ResourceMode::Selected,
                                resources,
                            )
                            .await
                            .map_err(source_error)
                    } else {
                        for connector in REMOTE_CALENDAR_CONNECTORS {
                            let current = service
                                .list_current(
                                    person_id,
                                    &ConnectorId::try_new(connector)
                                        .expect("constant connector ID"),
                                )
                                .await
                                .map_err(source_error)?;
                            if current.iter().any(|source| {
                                source.execution_owner_id().as_str() == caller.device_id()
                            }) {
                                return Err(CoreError::new(
                                    ErrorCode::Conflict,
                                    "disconnect the current remote Calendar source first",
                                ));
                            }
                        }
                        if service
                            .load(person_id, &connection_id)
                            .await
                            .map_err(source_error)?
                            .is_some()
                        {
                            return Err(CoreError::new(
                                ErrorCode::Conflict,
                                "remote Calendar source already exists",
                            ));
                        }
                        service
                            .establish(
                                person_id,
                                connector_id,
                                connection_id,
                                ExecutionOwnerId::try_new(caller.device_id()).map_err(|_| {
                                    CoreError::new(ErrorCode::Validation, "invalid device identity")
                                })?,
                                ResourceMode::Selected,
                                resources,
                            )
                            .await
                            .map_err(source_error)
                    }
                }
                RemoteCalendarSourceMutation::Disconnect {
                    connection_id,
                    expected_revision,
                } => {
                    verify_remote_source(&service, person_id, caller.device_id(), &connection_id)
                        .await?;
                    service
                        .disconnect(person_id, &connection_id, expected_revision)
                        .await
                        .map_err(source_error)
                }
            }
        })
    }
}

async fn verify_remote_source<Repository: floe_connections::SourceRepository + ?Sized>(
    service: &floe_connections::SourceConnectionService<'_, Repository>,
    person_id: PersonId,
    device_id: &str,
    connection_id: &ConnectionId,
) -> Result<SourceConnection, CoreError> {
    let source = service
        .load(person_id, connection_id)
        .await
        .map_err(source_error)?
        .ok_or_else(|| CoreError::new(ErrorCode::NotFound, "remote Calendar source not found"))?;
    if !REMOTE_CALENDAR_CONNECTORS.contains(&source.connector_id().as_str())
        || source.execution_owner_id().as_str() != device_id
    {
        return Err(CoreError::new(
            ErrorCode::NotFound,
            "remote Calendar source not found",
        ));
    }
    Ok(source)
}

pub enum NativeCalendarSourceMutation {
    Establish {
        resource_mode: ResourceMode,
        resources: Vec<ConnectionResource>,
    },
    Configure {
        connection_id: ConnectionId,
        expected_revision: u64,
        resource_mode: ResourceMode,
        resources: Vec<ConnectionResource>,
    },
    ReconcileInventory {
        connection_id: ConnectionId,
        expected_revision: u64,
        resources: Vec<ConnectionResource>,
    },
    Disconnect {
        connection_id: ConnectionId,
        expected_revision: u64,
    },
}

pub trait NativeCalendarSourceCommands {
    fn inspect_native_calendar_source(
        &self,
        caller: &CallerContext,
    ) -> Result<Option<SourceConnection>, CoreError>;

    fn mutate_native_calendar_source(
        &self,
        caller: &CallerContext,
        mutation: NativeCalendarSourceMutation,
    ) -> Result<SourceConnection, CoreError>;
}

impl NativeCalendarSourceCommands for AppComposition {
    fn inspect_native_calendar_source(
        &self,
        caller: &CallerContext,
    ) -> Result<Option<SourceConnection>, CoreError> {
        let person_id = PersonId(caller.person_id());
        let connector_id = native_connector();
        let sources = self
            .runtime
            .block_on(
                self.core
                    .source_service()
                    .list_current(person_id, &connector_id),
            )
            .map_err(source_error)?;
        let mut owned = sources
            .into_iter()
            .filter(|source| source.execution_owner_id().as_str() == caller.device_id());
        let source = owned.next();
        if owned.next().is_some() {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "multiple current native Calendar sources",
            ));
        }
        Ok(source)
    }

    fn mutate_native_calendar_source(
        &self,
        caller: &CallerContext,
        mutation: NativeCalendarSourceMutation,
    ) -> Result<SourceConnection, CoreError> {
        let person_id = PersonId(caller.person_id());
        let service = self.core.source_service();
        let subject = crate::vault_host::calendar_access::DeviceCalendarSubject {
            local_context: &self.local_context,
        };
        self.runtime.block_on(async {
            match mutation {
                NativeCalendarSourceMutation::Establish {
                    resource_mode,
                    resources,
                } => {
                    let existing = service
                        .list_current(person_id, &native_connector())
                        .await
                        .map_err(source_error)?;
                    if existing
                        .iter()
                        .any(|source| source.execution_owner_id().as_str() == caller.device_id())
                    {
                        return Err(CoreError::new(
                            ErrorCode::Conflict,
                            "disconnect the current native Calendar source first",
                        ));
                    }
                    let connection_id = ConnectionId::new();
                    let fingerprint = probe_calendar_source(
                        &subject,
                        person_id,
                        caller.device_id(),
                        &connection_id,
                        1,
                        &resources,
                    )
                    .await?;
                    service
                        .establish_reviewed_native(
                            person_id,
                            native_connector(),
                            connection_id,
                            ExecutionOwnerId::try_new(caller.device_id()).map_err(|_| {
                                CoreError::new(ErrorCode::Validation, "invalid device identity")
                            })?,
                            resource_mode,
                            resources,
                            fingerprint,
                        )
                        .await
                        .map_err(source_error)
                }
                NativeCalendarSourceMutation::Configure {
                    connection_id,
                    expected_revision,
                    resource_mode,
                    resources,
                } => {
                    verify_native_source(&service, person_id, caller.device_id(), &connection_id)
                        .await?;
                    let fingerprint = probe_calendar_source(
                        &subject,
                        person_id,
                        caller.device_id(),
                        &connection_id,
                        expected_revision,
                        &resources,
                    )
                    .await?;
                    service
                        .configure_reviewed_native(
                            person_id,
                            &connection_id,
                            expected_revision,
                            resource_mode,
                            resources,
                            fingerprint,
                        )
                        .await
                        .map_err(source_error)
                }
                NativeCalendarSourceMutation::ReconcileInventory {
                    connection_id,
                    expected_revision,
                    resources,
                } => {
                    verify_native_source(&service, person_id, caller.device_id(), &connection_id)
                        .await?;
                    let source = service
                        .load(person_id, &connection_id)
                        .await
                        .map_err(source_error)?
                        .ok_or_else(|| {
                            CoreError::new(ErrorCode::NotFound, "native Calendar source not found")
                        })?;
                    let fingerprint = probe_calendar_source(
                        &subject,
                        person_id,
                        caller.device_id(),
                        &connection_id,
                        expected_revision,
                        &resources,
                    )
                    .await?;
                    service
                        .configure_reviewed_native(
                            person_id,
                            &connection_id,
                            expected_revision,
                            source.resource_mode(),
                            resources,
                            fingerprint,
                        )
                        .await
                        .map_err(source_error)
                }
                NativeCalendarSourceMutation::Disconnect {
                    connection_id,
                    expected_revision,
                } => {
                    verify_native_source(&service, person_id, caller.device_id(), &connection_id)
                        .await?;
                    service
                        .disconnect(person_id, &connection_id, expected_revision)
                        .await
                        .map_err(source_error)
                }
            }
        })
    }
}

async fn probe_calendar_source(
    subject: &impl NativeCalendarSubjectSource,
    person_id: PersonId,
    device_id: &str,
    connection_id: &ConnectionId,
    connection_revision: u64,
    resources: &[ConnectionResource],
) -> Result<String, CoreError> {
    let mut calendar_ids: Vec<String> = resources
        .iter()
        .map(|resource| resource.handle().as_str().to_owned())
        .collect();
    calendar_ids.sort();
    if calendar_ids.is_empty()
        || calendar_ids
            .windows(2)
            .any(|neighbors| neighbors[0] == neighbors[1])
    {
        return Err(CoreError::new(
            ErrorCode::Validation,
            "invalid Calendar resources",
        ));
    }
    let observed = subject
        .subject(NativeSubjectRequest {
            person_id,
            device_id: device_id.to_owned(),
            provider: CalendarProvider::EventKit,
            calendar_ids,
            connection_id: connection_id.as_str().to_owned(),
            connection_revision,
            window: crate::vault_host::calendar_access::subject_window(
                floe_execution::Cancellation::default(),
            ),
        })
        .await
        .map_err(|_| CoreError::new(ErrorCode::Validation, "native Calendar unavailable"))?;
    if observed
        .after
        .as_ref()
        .is_some_and(|after| after != &observed.before)
        || !valid_subject_fingerprint(&observed.before)
    {
        return Err(CoreError::new(
            ErrorCode::Conflict,
            "native Calendar subject changed",
        ));
    }
    Ok(observed.before)
}

async fn verify_native_source<Repository: floe_connections::SourceRepository + ?Sized>(
    service: &floe_connections::SourceConnectionService<'_, Repository>,
    person_id: PersonId,
    device_id: &str,
    connection_id: &ConnectionId,
) -> Result<(), CoreError> {
    let source = service
        .load(person_id, connection_id)
        .await
        .map_err(source_error)?
        .ok_or_else(|| CoreError::new(ErrorCode::NotFound, "native Calendar source not found"))?;
    if source.connector_id() != &native_connector()
        || source.execution_owner_id().as_str() != device_id
    {
        return Err(CoreError::new(
            ErrorCode::NotFound,
            "native Calendar source not found",
        ));
    }
    Ok(())
}

fn native_connector() -> ConnectorId {
    ConnectorId::try_new(EVENT_KIT_CONNECTOR).expect("constant connector ID")
}

pub(crate) fn source_error(error: SourceServiceError) -> CoreError {
    let code = match error {
        SourceServiceError::NotFound => ErrorCode::NotFound,
        SourceServiceError::Invalid(
            SourceConnectionError::Conflict | SourceConnectionError::Disconnected,
        )
        | SourceServiceError::Repository(SourceRepositoryError::Conflict) => ErrorCode::Conflict,
        SourceServiceError::Invalid(_) => ErrorCode::Validation,
        SourceServiceError::Repository(_) => ErrorCode::Storage,
    };
    CoreError::new(code, error.to_string())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePersonalSourceSetup {
    pub connector_id: String,
    pub expected_revision: Option<u64>,
    pub selected_handles: Vec<String>,
}

pub trait NativePersonalSourceCommands {
    fn inspect_native_personal_source(
        &self,
        caller: &CallerContext,
        connector_id: &str,
    ) -> Result<Option<SourceConnection>, CoreError>;

    fn setup_native_personal_source(
        &self,
        caller: &CallerContext,
        setup: NativePersonalSourceSetup,
    ) -> Result<SourceConnection, CoreError>;
}

impl NativePersonalSourceCommands for AppComposition {
    fn inspect_native_personal_source(
        &self,
        caller: &CallerContext,
        connector_id: &str,
    ) -> Result<Option<SourceConnection>, CoreError> {
        let spec = PersonalSourceSpec::for_connector(connector_id)
            .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid personal connector"))?;
        let connection_id = ConnectionId::try_new(spec.connection)
            .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid connection identity"))?;
        let owner = spec
            .execution_owner(caller.device_id())
            .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid device identity"))?;
        let source = self
            .runtime
            .block_on(
                self.core
                    .source_service()
                    .load(PersonId(caller.person_id()), &connection_id),
            )
            .map_err(source_error)?;
        if source.as_ref().is_some_and(|source| {
            source.connector_id().as_str() != spec.connector
                || source.execution_owner_id().as_str() != owner
        }) {
            return Err(CoreError::new(
                ErrorCode::NotFound,
                "personal source not found",
            ));
        }
        Ok(source)
    }

    fn setup_native_personal_source(
        &self,
        caller: &CallerContext,
        setup: NativePersonalSourceSetup,
    ) -> Result<SourceConnection, CoreError> {
        let inspector = floe_provider_adapters::sources::NativePersonalDriver {
            attention: self.local_context.attention(),
            personal: self.local_context.personal(),
            observations: self.local_context.observations(),
        };
        self.runtime.block_on(setup_personal_source(
            self.core.as_ref(),
            &inspector,
            PersonId(caller.person_id()),
            caller.device_id(),
            setup,
        ))
    }
}

async fn setup_personal_source(
    core: &crate::FloeCore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    device_id: &str,
    setup: NativePersonalSourceSetup,
) -> Result<SourceConnection, CoreError> {
    let spec = PersonalSourceSpec::for_connector(&setup.connector_id)
        .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid personal connector"))?;
    let resources = spec
        .resources(setup.selected_handles)
        .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid source resources"))?;
    let owner = spec
        .execution_owner(device_id)
        .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid device identity"))?;
    let connection_id = ConnectionId::try_new(spec.connection)
        .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid connection identity"))?;
    if setup
        .expected_revision
        .is_some_and(|revision| revision == 0)
    {
        return Err(CoreError::new(
            ErrorCode::Validation,
            "invalid source revision",
        ));
    }
    let service = core.source_service();
    let existing = service
        .load(person_id, &connection_id)
        .await
        .map_err(source_error)?;
    match (setup.expected_revision, existing.as_ref()) {
        (Some(expected_revision), Some(current))
            if current.connector_id().as_str() == spec.connector
                && current.execution_owner_id().as_str() == owner
                && current.revision() == expected_revision => {}
        (None, None) => {}
        _ => {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "personal source changed",
            ))
        }
    }
    let probe = match spec.connector {
        "contacts.apple" | "contacts.android" => PersonalSubjectProbe::People {
            selected_handles: resources
                .iter()
                .map(|resource| resource.handle().as_str().to_owned())
                .collect(),
        },
        "attention.macos" => PersonalSubjectProbe::Attention,
        "health.apple" => PersonalSubjectProbe::Wellbeing,
        _ => {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "invalid personal connector",
            ))
        }
    };
    let evidence = inspector
        .inspect(
            person_id,
            device_id,
            probe,
            None,
            Some(tokio::time::Instant::now() + std::time::Duration::from_secs(30)),
            floe_execution::Cancellation::default(),
        )
        .await
        .map_err(|_| CoreError::new(ErrorCode::Validation, "native source unavailable"))?;
    if evidence.before != evidence.after || !valid_subject_fingerprint(&evidence.before) {
        return Err(CoreError::new(
            ErrorCode::Conflict,
            "native subject changed",
        ));
    }
    match setup.expected_revision {
        Some(expected_revision) => service
            .configure_reviewed_native(
                person_id,
                &connection_id,
                expected_revision,
                spec.mode,
                resources,
                evidence.before,
            )
            .await
            .map_err(source_error),
        None => service
            .establish_reviewed_native(
                person_id,
                ConnectorId::try_new(spec.connector)
                    .map_err(|_| CoreError::new(ErrorCode::Validation, "invalid connector"))?,
                connection_id,
                ExecutionOwnerId::try_new(owner).map_err(|_| {
                    CoreError::new(ErrorCode::Validation, "invalid device identity")
                })?,
                spec.mode,
                resources,
                evidence.before,
            )
            .await
            .map_err(source_error),
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionsResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub connections: Option<Vec<floe_connections::ConnectorSnapshot>>,
    pub failure: Option<AgentFailure>,
}

pub trait ConnectionsQueries {
    fn inspect_connections(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<ConnectionsResult, ServiceError>;
    fn read_connections_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ConnectionsResult, ServiceError>;
}

impl ConnectionsQueries for AppComposition {
    fn inspect_connections(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
    ) -> Result<ConnectionsResult, ServiceError> {
        self.connections_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::Connections),
            false,
        )
    }
    fn read_connections_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<ConnectionsResult, ServiceError> {
        self.connections_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn connections_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<ConnectionsResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Connections,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(ConnectionsResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            connections: result.connections,
            failure: result.failure,
        })
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use floe_access::{GrantId, GrantScope, GrantSourceBinding, PersonalSubjectEvidence};
    use floe_agent_contract::BoxFuture;
    use floe_context_contract::ResourceHandle;
    use floe_vault::{AccessGrantActivation, EncryptedAgentVault, VaultKey, VaultKeyProvider};
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Subject {
        probes: Mutex<Vec<Vec<String>>>,
    }

    impl PersonalSubjectInspector for Subject {
        fn inspect<'a>(
            &'a self,
            _person_id: PersonId,
            _device_id: &'a str,
            probe: PersonalSubjectProbe<'a>,
            _expected_native_subject_fingerprint: Option<String>,
            _deadline: Option<tokio::time::Instant>,
            _cancellation: floe_execution::Cancellation,
        ) -> BoxFuture<'a, Result<PersonalSubjectEvidence, AgentFailure>> {
            Box::pin(async move {
                if let PersonalSubjectProbe::People { selected_handles } = probe {
                    self.probes.lock().unwrap().push(selected_handles);
                }
                Ok(PersonalSubjectEvidence {
                    before: "a".repeat(64),
                    after: "a".repeat(64),
                })
            })
        }

        fn attention_presence(&self, _person_id: PersonId, _device_id: &str) -> Option<Uuid> {
            None
        }
    }

    struct CalendarSubject {
        probes: Mutex<Vec<Vec<String>>>,
        after: Option<String>,
    }

    impl NativeCalendarSubjectSource for CalendarSubject {
        async fn subject(
            &self,
            request: NativeSubjectRequest,
        ) -> Result<floe_context::NativeSubjectObservation, AgentFailure> {
            self.probes.lock().unwrap().push(request.calendar_ids);
            Ok(floe_context::NativeSubjectObservation {
                before: "a".repeat(64),
                after: self.after.clone(),
            })
        }
    }

    #[tokio::test]
    async fn native_calendar_source_probe_handles_eleven_ids_and_rejects_drift() {
        let subject = CalendarSubject {
            probes: Mutex::new(Vec::new()),
            after: None,
        };
        let resources: Vec<_> = (0..11)
            .rev()
            .map(|index| {
                ConnectionResource::new(
                    ResourceHandle::try_new(format!("calendar-{index:02}")).unwrap(),
                    format!("Calendar {index}"),
                )
                .unwrap()
            })
            .collect();
        let fingerprint = probe_calendar_source(
            &subject,
            PersonId::new(),
            "mac-local",
            &ConnectionId::new(),
            1,
            &resources,
        )
        .await
        .unwrap();
        assert_eq!(fingerprint, "a".repeat(64));
        let probes = subject.probes.lock().unwrap();
        assert_eq!(probes[0].len(), 11);
        assert_eq!(probes[0].first().unwrap(), "calendar-00");
        drop(probes);
        let drifted = CalendarSubject {
            probes: Mutex::new(Vec::new()),
            after: Some("b".repeat(64)),
        };
        assert_eq!(
            probe_calendar_source(
                &drifted,
                PersonId::new(),
                "mac-local",
                &ConnectionId::new(),
                1,
                &resources,
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::Conflict
        );
    }

    #[derive(Clone, Default)]
    struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for Keys {
        fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person, vault))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person: PersonId,
            vault: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person, vault), *key.as_bytes());
            Ok(())
        }
    }

    #[tokio::test]
    async fn contacts_source_edit_changes_only_source_authority() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let core = crate::FloeCore::open(directory.path().join("personal-source.db"))
            .await
            .unwrap();
        let person = PersonId::new();
        let vault = EncryptedAgentVault::create(directory.path(), person, Keys::default())
            .await
            .unwrap();
        let subject = Subject::default();
        let setup = |expected_revision, selected_handles: &[&str]| NativePersonalSourceSetup {
            connector_id: "contacts.apple".into(),
            expected_revision,
            selected_handles: selected_handles
                .iter()
                .map(|handle| (*handle).into())
                .collect(),
        };
        let initial =
            setup_personal_source(&core, &subject, person, "device-1", setup(None, &["A"]))
                .await
                .unwrap();
        let policy = crate::first_party_observe::personal_policy("contacts.apple").unwrap();
        let binding = GrantSourceBinding::try_new(
            person,
            initial.connection_id().clone(),
            initial.connector_id().clone(),
            initial.execution_owner_id().clone(),
        )
        .unwrap();
        let logical = floe_context_contract::connection_view_resource(
            policy.view_id,
            initial.connection_id(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![logical.clone()],
            policy.categories,
            vec![policy.operation],
            vec![policy.purpose],
            policy.consumers,
            policy.processing,
        )
        .unwrap();
        let [grant] = vault
            .activate_access_grants(vec![AccessGrantActivation {
                grant_id: GrantId::new(),
                expected: None,
                source: binding.clone(),
                scope,
            }])
            .await
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(
            setup_personal_source(
                &core,
                &subject,
                person,
                "device-1",
                setup(Some(initial.revision()), &["A", "A"])
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::Validation
        );
        assert_eq!(subject.probes.lock().unwrap().len(), 1);
        let changed = setup_personal_source(
            &core,
            &subject,
            person,
            "device-1",
            setup(Some(initial.revision()), &["A", "B"]),
        )
        .await
        .unwrap();
        assert_eq!(changed.revision(), initial.revision() + 1);
        assert_eq!(
            changed.source_authority(),
            initial.source_authority().advance().unwrap()
        );
        assert_eq!(
            changed
                .resources()
                .iter()
                .map(|resource| resource.handle().as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        let current_grant = vault
            .data_access_grant_for_source_resource(&binding, &logical)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current_grant.id(), grant.id());
        assert_eq!(current_grant.authority(), grant.authority());
        assert_eq!(current_grant.state(), floe_access::GrantState::Active);
        assert_eq!(
            *subject.probes.lock().unwrap(),
            [vec!["A".to_owned()], vec!["A".to_owned(), "B".to_owned()]]
        );
    }

    #[tokio::test]
    async fn native_source_mutation_rejects_foreign_device() {
        let directory = tempfile::tempdir().unwrap();
        let core = crate::FloeCore::open(directory.path().join("native-source.db"))
            .await
            .unwrap();
        let service = core.source_service();
        let person_id = PersonId::new();
        let source = service
            .establish(
                person_id,
                native_connector(),
                ConnectionId::new(),
                ExecutionOwnerId::try_new("mac-local").unwrap(),
                ResourceMode::Selected,
                vec![ConnectionResource::new(
                    ResourceHandle::try_new("home").unwrap(),
                    "Home".into(),
                )
                .unwrap()],
            )
            .await
            .unwrap();
        verify_native_source(&service, person_id, "mac-local", source.connection_id())
            .await
            .unwrap();
        assert_eq!(
            verify_native_source(
                &service,
                person_id,
                "foreign-device",
                source.connection_id()
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            verify_native_source(
                &service,
                PersonId::new(),
                "mac-local",
                source.connection_id()
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn remote_calendar_resource_set_advances_local_source_authority_once() {
        let directory = tempfile::tempdir().unwrap();
        let core = crate::FloeCore::open(directory.path().join("remote-source.db"))
            .await
            .unwrap();
        let service = core.source_service();
        let person_id = PersonId::new();
        let resource = |handle: &str| {
            ConnectionResource::new(ResourceHandle::try_new(handle).unwrap(), handle.into())
                .unwrap()
        };
        let initial = service
            .establish(
                person_id,
                ConnectorId::try_new("calendar.google").unwrap(),
                ConnectionId::new(),
                ExecutionOwnerId::try_new("server-owner").unwrap(),
                ResourceMode::Selected,
                vec![resource("A")],
            )
            .await
            .unwrap();
        let expanded = service
            .configure(
                person_id,
                initial.connection_id(),
                initial.revision(),
                ResourceMode::Selected,
                vec![resource("B"), resource("A")],
            )
            .await
            .unwrap();
        assert_eq!(expanded.revision(), initial.revision() + 1);
        assert_eq!(
            expanded.source_authority().incarnation(),
            initial.source_authority().incarnation()
        );
        assert_eq!(
            expanded.source_authority().epoch().get(),
            initial.source_authority().epoch().get() + 1
        );
        assert_eq!(
            expanded
                .resources()
                .iter()
                .map(|item| item.handle().as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        let reordered = service
            .configure(
                person_id,
                initial.connection_id(),
                expanded.revision(),
                ResourceMode::Selected,
                vec![resource("A"), resource("B")],
            )
            .await
            .unwrap();
        assert_eq!(reordered.revision(), expanded.revision());
        assert_eq!(reordered.source_authority(), expanded.source_authority());
    }
}
