//! Choosing the remote inference route for one verified caller.
//!
//! Selection is a policy decision: whether a saved connection exists at all,
//! whether it may be bound to this caller, and only then what the paired server
//! reports. The credential store and the server conversation are I/O and stay
//! behind these owner-defined ports; the composition root injects them.

use floe_agent_contract::{AgentFailure, BoxFuture};

use crate::{RemoteModelConnection, SavedServerConnection, admit_saved_connection};

/// Where this host keeps the saved local-server connection.
pub trait SavedConnectionStore {
    fn load(&self) -> Result<Option<SavedServerConnection>, AgentFailure>;
}

/// Who turns an admitted connection into a concrete route by asking the server.
///
/// The route shape belongs to the caller's own boundary, so it stays generic
/// here: Inference decides *whether* to resolve, never how a route is encoded.
pub trait RemoteRouteResolver<Route> {
    fn resolve<'a>(
        &'a self,
        connection: &'a RemoteModelConnection,
    ) -> BoxFuture<'a, Result<Route, AgentFailure>>;
}

/// Select the remote route for this caller, or none when the host has no saved
/// connection.
///
/// A saved connection is never used as stored: it is admitted against the
/// verified caller identity and its recorded consent first, and a connection
/// that fails admission is denied rather than downgraded to a local route.
pub async fn select_remote_route<Route, Store, Resolver>(
    store: &Store,
    resolver: &Resolver,
    person_id: &str,
    device_id: &str,
) -> Result<Option<Route>, AgentFailure>
where
    Store: SavedConnectionStore + ?Sized,
    Resolver: RemoteRouteResolver<Route> + ?Sized,
{
    let Some(saved) = store.load()? else {
        return Ok(None);
    };
    let connection = admit_saved_connection(saved, person_id, device_id)?;
    resolver.resolve(&connection).await.map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERSON: &str = "00000000-0000-4000-8000-000000000001";
    const DEVICE: &str = "local-device";

    struct Store(Option<SavedServerConnection>);

    impl SavedConnectionStore for Store {
        fn load(&self) -> Result<Option<SavedServerConnection>, AgentFailure> {
            Ok(self.0.clone())
        }
    }

    struct Resolver;

    impl RemoteRouteResolver<String> for Resolver {
        fn resolve<'a>(
            &'a self,
            connection: &'a RemoteModelConnection,
        ) -> BoxFuture<'a, Result<String, AgentFailure>> {
            Box::pin(async move { Ok(connection.base_url.clone()) })
        }
    }

    fn saved() -> SavedServerConnection {
        SavedServerConnection {
            base_url: "http://127.0.0.1:8431".into(),
            token: "a".repeat(32),
            client_id: "paired-client".into(),
            person_id: PERSON.into(),
            device_id: DEVICE.into(),
            allow_external: false,
            external_recipients: vec![],
        }
    }

    #[tokio::test]
    async fn no_saved_connection_selects_no_remote_route() {
        assert_eq!(
            select_remote_route(&Store(None), &Resolver, PERSON, DEVICE).await,
            Ok(None)
        );
    }

    #[tokio::test]
    async fn a_saved_connection_is_admitted_before_it_is_resolved() {
        assert_eq!(
            select_remote_route(&Store(Some(saved())), &Resolver, PERSON, DEVICE).await,
            Ok(Some("http://127.0.0.1:8431".to_owned()))
        );
        assert_eq!(
            select_remote_route(&Store(Some(saved())), &Resolver, PERSON, "other-device").await,
            Err(AgentFailure::PolicyDenied)
        );
    }
}
