use floe_agent_contract::{AGENT_VERSION, AgentFailure, BoxFuture, PersonId};
use floe_context_contract::{
    CalendarViewQuery, ContextDependency, GrantConsumer, GrantPurpose, SourceReadOutcome,
};
use floe_execution::Cancellation;
use serde_json::Value;
use tokio::time::Instant;

use crate::{ContextService, SourceReader, SourceView};

pub struct DeclaredSourceRequirement<'a> {
    pub key: &'a str,
    pub capability: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalExpertSource {
    Calendar,
    People,
    Attention,
    Wellbeing,
    ConfirmedInteractions,
    ConfirmedMemory,
    Tasks,
}

pub trait LocalExpertSourceDriver: Sync {
    fn read<'a>(
        &'a self,
        source: LocalExpertSource,
        query: Value,
        deadline: Instant,
        cancellation: &'a Cancellation,
    ) -> BoxFuture<'a, Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>>;
}

pub struct DeclaredSourceValue {
    pub payload: Value,
    pub dependencies: Vec<ContextDependency>,
    pub held: Option<SourceView<Value>>,
}

pub async fn read_declared_source(
    remote_reader: Option<&dyn SourceReader>,
    local_driver: &dyn LocalExpertSourceDriver,
    person_id: PersonId,
    consumer: &str,
    requirements: &[DeclaredSourceRequirement<'_>],
    key: &str,
    query: Value,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<SourceReadOutcome<DeclaredSourceValue>, AgentFailure> {
    let requirement = requirements
        .iter()
        .find(|requirement| requirement.key == key)
        .ok_or(AgentFailure::CapabilityDenied)?;
    let local = match requirement.capability {
        "calendar.timeline" => Some(LocalExpertSource::Calendar),
        "people.identity" => Some(LocalExpertSource::People),
        "attention.coarse" => Some(LocalExpertSource::Attention),
        "wellbeing.derived" => Some(LocalExpertSource::Wellbeing),
        "relationships.confirmed_interactions" => Some(LocalExpertSource::ConfirmedInteractions),
        "memory.confirmed" => Some(LocalExpertSource::ConfirmedMemory),
        "floe.tasks" => Some(LocalExpertSource::Tasks),
        "mail.communication" | "work.context" | "life.logistics" => None,
        _ => return Err(AgentFailure::CapabilityUnavailable),
    };
    if let Some(source) = local {
        if source == LocalExpertSource::Calendar {
            let calendar: CalendarViewQuery =
                serde_json::from_value(query.clone()).map_err(|_| AgentFailure::InvalidInput)?;
            calendar.validate()?;
        } else if source != LocalExpertSource::ConfirmedInteractions
            && query != serde_json::json!({"schema_version": AGENT_VERSION})
        {
            return Err(AgentFailure::InvalidInput);
        }
        if serde_json::to_vec(&query)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > 65_536
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let acquired = match local_driver
            .read(source, query, deadline, cancellation)
            .await
        {
            Err(AgentFailure::CapabilityUnavailable)
                if source == LocalExpertSource::ConfirmedInteractions =>
            {
                return Ok(SourceReadOutcome::Ready(DeclaredSourceValue {
                    payload: serde_json::json!([]),
                    dependencies: vec![],
                    held: None,
                }));
            }
            result => result?,
        };
        return Ok(match acquired {
            SourceReadOutcome::Ready((payload, dependencies)) => {
                SourceReadOutcome::Ready(DeclaredSourceValue {
                    payload,
                    dependencies,
                    held: None,
                })
            }
            SourceReadOutcome::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
            SourceReadOutcome::NeedsUserAction(blockers) => {
                SourceReadOutcome::NeedsUserAction(blockers)
            }
        });
    }
    Ok(
        match read_declared_remote_source(
            remote_reader,
            person_id,
            consumer,
            requirements,
            key,
            query,
            deadline,
            cancellation,
        )
        .await?
        {
            SourceReadOutcome::Ready(read) => SourceReadOutcome::Ready(DeclaredSourceValue {
                payload: read.payload().clone(),
                dependencies: read
                    .bindings()
                    .iter()
                    .map(|binding| binding.dependency.clone())
                    .collect(),
                held: Some(read),
            }),
            SourceReadOutcome::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
            SourceReadOutcome::NeedsUserAction(blockers) => {
                SourceReadOutcome::NeedsUserAction(blockers)
            }
        },
    )
}

async fn read_declared_remote_source(
    reader: Option<&dyn SourceReader>,
    person_id: PersonId,
    consumer: &str,
    requirements: &[DeclaredSourceRequirement<'_>],
    key: &str,
    query: Value,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<SourceReadOutcome<SourceView<Value>>, AgentFailure> {
    let requirement = requirements
        .iter()
        .find(|requirement| requirement.key == key)
        .ok_or(AgentFailure::CapabilityDenied)?;
    if !matches!(
        requirement.capability,
        "mail.communication" | "work.context" | "life.logistics"
    ) {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    crate::validate_remote_view_query(requirement.capability, &query)?;
    let prepared = ContextService::new(reader).prepare(person_id)?;
    let request = prepared.source_request(
        requirement.capability,
        GrantConsumer::builtin(consumer).map_err(|_| AgentFailure::InvalidInput)?,
        GrantPurpose::Assistant,
        query,
        deadline,
        cancellation.clone(),
    )?;
    prepared.read_source(&request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct ProbeDriver(Mutex<Vec<LocalExpertSource>>);

    impl LocalExpertSourceDriver for ProbeDriver {
        fn read<'a>(
            &'a self,
            source: LocalExpertSource,
            _: Value,
            _: Instant,
            _: &'a Cancellation,
        ) -> BoxFuture<'a, Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>>
        {
            Box::pin(async move {
                self.0.lock().unwrap().push(source);
                Ok(SourceReadOutcome::Ready((serde_json::json!([]), vec![])))
            })
        }
    }

    #[tokio::test]
    async fn declared_local_capability_selects_only_its_driver() {
        let driver = ProbeDriver(Mutex::new(vec![]));
        let requirements = [DeclaredSourceRequirement {
            key: "calendar",
            capability: "calendar.timeline",
        }];
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let cancellation = Cancellation::default();
        let query = serde_json::json!({"range_start_unix_ms": 1, "range_end_unix_ms": 1000, "cursor": null, "limit": 1});
        assert!(matches!(
            read_declared_source(
                None,
                &driver,
                PersonId::new(),
                "example.test.expert",
                &requirements,
                "calendar",
                query.clone(),
                deadline,
                &cancellation
            )
            .await,
            Ok(SourceReadOutcome::Ready(_))
        ));
        assert_eq!(*driver.0.lock().unwrap(), vec![LocalExpertSource::Calendar]);
        assert!(matches!(
            read_declared_source(
                None,
                &driver,
                PersonId::new(),
                "example.test.expert",
                &requirements,
                "other",
                query,
                deadline,
                &cancellation
            )
            .await,
            Err(AgentFailure::CapabilityDenied)
        ));
        assert_eq!(driver.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn undeclared_key_and_unbounded_query_fail_before_source_selection() {
        let requirements = [DeclaredSourceRequirement {
            key: "mail",
            capability: "mail.communication",
        }];
        let person = PersonId::new();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let cancellation = Cancellation::default();
        assert!(matches!(
            read_declared_remote_source(
                None,
                person,
                "example.test.expert",
                &requirements,
                "work",
                serde_json::json!({}),
                deadline,
                &cancellation
            )
            .await,
            Err(AgentFailure::CapabilityDenied)
        ));
        assert!(matches!(
            read_declared_remote_source(
                None,
                person,
                "example.test.expert",
                &requirements,
                "mail",
                serde_json::json!({"limit": 100_000}),
                deadline,
                &cancellation
            )
            .await,
            Err(AgentFailure::InvalidInput)
        ));
    }
}
