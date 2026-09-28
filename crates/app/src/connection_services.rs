use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CallerContext, CoreError, ErrorCode, ServiceError, VaultState,
};
use floe_connections::{
    ConnectionId, ConnectionResource, ConnectorId, ResourceMode, SourceConnection,
    SourceConnectionError, SourceRepositoryError, SourceServiceError,
};
use floe_context_contract::ExecutionOwnerId;
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
                    service
                        .establish(
                            person_id,
                            native_connector(),
                            ConnectionId::new(),
                            ExecutionOwnerId::try_new(caller.device_id()).map_err(|_| {
                                CoreError::new(ErrorCode::Validation, "invalid device identity")
                            })?,
                            resource_mode,
                            resources,
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
                    service
                        .configure(
                            person_id,
                            &connection_id,
                            expected_revision,
                            resource_mode,
                            resources,
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
                    service
                        .reconcile_inventory(
                            person_id,
                            &connection_id,
                            expected_revision,
                            resources,
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
    use floe_context_contract::ResourceHandle;

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
                vec![
                    ConnectionResource::new(
                        ResourceHandle::try_new("home").unwrap(),
                        "Home".into(),
                    )
                    .unwrap(),
                ],
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
            ConnectionResource::new(
                ResourceHandle::try_new(handle).unwrap(),
                handle.into(),
            )
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
            expanded.resources().iter().map(|item| item.handle().as_str()).collect::<Vec<_>>(),
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
