//! Reauthorization of retained source evidence using owner-injected capabilities.
use super::source_adapters::{
    CalendarConnections, NativeCalendar, NativeGrants, PersonalConnections,
};
use crate::{ExpertSourceTransport, PersonalSourceDriver, SourceLeaseRegistry};
use floe_access::{
    DependencyAuthorization, DependencyResolver, GrantRepository, RemoteCallWindow,
    RemotePairingIdentity,
};
use floe_connections::ConnectionsRepository;
use floe_context_contract::ContextDependency;
use floe_execution::budget::{BudgetConfig, BudgetLedger, ModelUsage};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor, TraceContext};
use std::sync::Arc;

pub struct ContextDependencyResolver {
    actor: OwnerActor,
    connections: Arc<dyn ConnectionsRepository>,
    grants: Arc<dyn GrantRepository>,
    personal: Arc<dyn PersonalSourceDriver + Send>,
    transport: Arc<dyn ExpertSourceTransport>,
    leases: Arc<SourceLeaseRegistry>,
}
impl ContextDependencyResolver {
    pub fn new(
        actor: OwnerActor,
        connections: Arc<dyn ConnectionsRepository>,
        grants: Arc<dyn GrantRepository>,
        personal: Arc<dyn PersonalSourceDriver + Send>,
        transport: Arc<dyn ExpertSourceTransport>,
        leases: Arc<SourceLeaseRegistry>,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        Ok(Self {
            actor,
            connections,
            grants,
            personal,
            transport,
            leases,
        })
    }
}
impl DependencyResolver for ContextDependencyResolver {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            if dependency.person_id() != self.actor.person_id
                || dependency.source().person_id() != self.actor.person_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let scope = ExecutionScope::root(
                request.cancellation.clone(),
                request.deadline,
                BudgetLedger::new(BudgetConfig::new(0, 0), ModelUsage::default()).root_lease(),
                TraceContext::new(uuid::Uuid::new_v4()),
            );
            scope
                .run(async {
                    let grants = self
                        .grants
                        .snapshot(dependency.source().clone())
                        .await?
                        .grants;
                    let grant = grants
                        .iter()
                        .find(|grant| grant.id() == dependency.grant_id())
                        .ok_or(AgentFailure::PolicyDenied)?;
                    floe_access::validate_grant_dependency(grant, dependency)?;
                    let connector = dependency.source().connector().as_str();
                    if matches!(connector, "calendar.event_kit" | "calendar.android") {
                        if dependency.source().execution_owner().as_str() != self.actor.device_id {
                            return Err(AgentFailure::PolicyDenied);
                        }
                        let connection_id = dependency.source().connection_id();
                        let connections = CalendarConnections {
                            repository: self.connections.as_ref(),
                            person_id: self.actor.person_id,
                            connection_id: &connection_id,
                        };
                        let connection = self
                            .connections
                            .load(self.actor.person_id, &connection_id)
                            .await
                            .map_err(|_| AgentFailure::StorageUnavailable)?
                            .ok_or(AgentFailure::StaleContext)?;
                        if connection.connector_id() != dependency.source().connector()
                            || connection.execution_owner_id()
                                != dependency.source().execution_owner()
                        {
                            return Err(AgentFailure::StaleContext);
                        }
                        let source = NativeCalendar {
                            actor: &self.actor,
                            connection: &connection,
                            transport: self.transport.as_ref(),
                        };
                        return crate::authorize_native_calendar_dependency(
                            &connections,
                            &source,
                            &NativeGrants(self.grants.as_ref()),
                            &self.leases,
                            dependency,
                            &RemoteCallWindow {
                                deadline: request.deadline,
                                cancellation: request.cancellation.clone(),
                            },
                        )
                        .await;
                    }
                    if floe_access::is_device_local_source(connector) {
                        return crate::authorize_personal_dependency(
                            &PersonalConnections(self.connections.as_ref()),
                            self.grants.as_ref(),
                            self.personal.as_ref(),
                            self.actor.person_id,
                            &self.actor.device_id,
                            dependency,
                            request.deadline,
                            &request.cancellation,
                        )
                        .await;
                    }
                    let remote = self
                        .transport
                        .remote(&self.actor, &scope)
                        .await?
                        .ok_or(AgentFailure::PolicyDenied)?;
                    let person = self.actor.person_id.to_string();
                    let pairing = RemotePairingIdentity {
                        person_id: &person,
                        device_id: &self.actor.device_id,
                        client_id: remote.transport.client_id(),
                    };
                    crate::authorize_remote_dependency(
                        self.grants.as_ref(),
                        remote.verifier.as_ref(),
                        self.connections.as_ref(),
                        self.connections.as_ref(),
                        remote.transport.as_ref(),
                        self.actor.person_id,
                        pairing,
                        dependency,
                        request,
                    )
                    .await
                })
                .await
        })
    }
}
