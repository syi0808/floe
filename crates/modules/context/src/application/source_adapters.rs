//! Owner-local views over the same exact injected source repositories and transports.
use crate::{
    CalendarConnectionReader, CalendarObserveRequest, CalendarSource, ExpertSourceTransport,
    NativeCalendarGrantReader, PersonalConnectionReader,
};
use floe_access::{GrantConsumer, GrantRepository};
use floe_connections::{ConnectionsRepository, SourceConnection};
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, OwnerActor, PersonId};

pub(crate) struct PersonalConnections<'a>(pub(crate) &'a dyn ConnectionsRepository);
impl PersonalConnectionReader for PersonalConnections<'_> {
    fn source_is_fenced<'a>(
        &'a self,
        person: PersonId,
        connection: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            self.0
                .source_is_fenced(person, connection)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)
        })
    }
    fn load<'a>(
        &'a self,
        person: PersonId,
        connection: &'a floe_context_contract::ConnectionId,
    ) -> BoxFuture<'a, Result<Option<SourceConnection>, AgentFailure>> {
        Box::pin(async move {
            self.0
                .load(person, connection)
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)
        })
    }
}

pub(crate) struct CalendarConnections<'a> {
    pub(crate) repository: &'a dyn ConnectionsRepository,
    pub(crate) person_id: PersonId,
    pub(crate) connection_id: &'a floe_context_contract::ConnectionId,
}
impl CalendarConnectionReader for CalendarConnections<'_> {
    async fn source_is_fenced(
        &self,
        person: PersonId,
        connection: &floe_context_contract::ConnectionId,
    ) -> Result<bool, AgentFailure> {
        self.repository
            .source_is_fenced(person, connection)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)
    }
    async fn calendar_connection(&self) -> Result<Option<SourceConnection>, AgentFailure> {
        self.repository
            .load(self.person_id, self.connection_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)
    }
}

pub(crate) struct NativeCalendar<'a> {
    pub(crate) actor: &'a OwnerActor,
    pub(crate) connection: &'a SourceConnection,
    pub(crate) transport: &'a dyn ExpertSourceTransport,
}
impl CalendarSource for NativeCalendar<'_> {
    async fn check(
        &self,
        request: floe_access::CalendarReadAccessRequest,
    ) -> Result<floe_access::CalendarReadAccessStamp, AgentFailure> {
        self.transport
            .check_calendar(self.actor, self.connection, request)
            .await
    }
    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<crate::CalendarObservation>, AgentFailure> {
        self.transport
            .observe_calendar(self.actor, self.connection, request)
            .await
            .map(Some)
    }
}
pub(crate) struct NativeGrants<'a>(pub(crate) &'a dyn GrantRepository);
impl NativeCalendarGrantReader for NativeGrants<'_> {
    async fn admit(
        &self,
        connection: &SourceConnection,
        person: PersonId,
        consumer: &str,
    ) -> Result<floe_access::CalendarReadAccessAdmission, AgentFailure> {
        let consumer =
            GrantConsumer::builtin(consumer).map_err(|_| AgentFailure::CapabilityDenied)?;
        let provider = floe_access::local_calendar_provider(connection.connector_id().as_str())
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        let grant = floe_access::current_native_calendar_grant(
            self.0,
            person,
            connection.connection_id().as_str(),
            provider,
            connection.execution_owner_id(),
            &consumer,
        )
        .await?;
        Ok(floe_access::CalendarReadAccessAdmission::device_local(
            person,
            grant.id(),
            grant.authority(),
            grant.source().clone(),
            connection.source_authority(),
            grant.scope().clone(),
            consumer,
        ))
    }
}
