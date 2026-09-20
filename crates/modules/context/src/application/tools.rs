//! The Manager tool catalog and its Context-owned implementation.
//!
//! Context is the source of truth for what the Manager may call: the seven
//! tool descriptors below, their canonical revisions and schemas, and the
//! source each tool reads. Descriptor availability never depends on the model
//! route: the same catalog is served whether the answering model runs on
//! device or remotely. Route/recipient admission happens later, when Access
//! model dispatch reauthorizes the dependency each successful result returns
//! directly.

use floe_agent_contract::{
    AgentFailure, BoxFuture, DependencyCoverage, PersonId, ToolCall, ToolDescriptor, ToolPort,
    ToolResult,
};
use floe_context_contract::{GrantConsumer, GrantPurpose};

use crate::application::service::{ContextService, PreparedContext};
use crate::ports::personal_source::{PersonalGrantRecords, PersonalSourceDriver};
use crate::ports::source_reader::SourceReader;
use crate::{ASSISTANT_CONSUMER, SourceView};

pub const PEOPLE_IDENTITY_READ: &str = "people.identity.read";
pub const SCHEDULE_FEASIBILITY_READ: &str = "schedule.feasibility.read";
pub const ATTENTION_COARSE_READ: &str = "attention.coarse.read";
pub const WELLBEING_DERIVED_READ: &str = "wellbeing.derived.read";
pub const MAIL_COMMUNICATION_READ: &str = "mail.communication.read";
pub const WORK_CONTEXT_READ: &str = "work.context.read";
pub const LIFE_LOGISTICS_READ: &str = "life.logistics.read";

/// The canonical definition revision every Manager tool is pinned at.
pub const MANAGER_TOOL_DEFINITION_REVISION: u64 = 1;

/// The canonical output data class every Manager tool declares: Manager tools
/// read the Person's own data.
const MANAGER_TOOL_OUTPUT_DATA_CLASS: &str = "personal";

/// Empty-object input shared by every Manager tool except mail search.
const EMPTY_INPUT_SCHEMA: &str =
    r#"{"type":"object","properties":{},"additionalProperties":false}"#;

/// Bounded mail search input: optional query/cursor/limit, unknown fields
/// rejected.
const MAIL_INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"query":{"type":"string","maxLength":512},"cursor":{"type":"integer","minimum":0,"maximum":10000},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#;

const MAX_MAIL_QUERY_CHARS: usize = 512;
const MAX_MAIL_CURSOR: usize = 10_000;
const MAX_MAIL_LIMIT: usize = 100;

fn default_communication_limit() -> usize {
    25
}

fn manager_tool(id: &str, description: &str, input_schema: &str) -> ToolDescriptor {
    ToolDescriptor {
        id: id.into(),
        definition_revision: MANAGER_TOOL_DEFINITION_REVISION,
        description: description.into(),
        input_schema: input_schema.into(),
        output_data_class: MANAGER_TOOL_OUTPUT_DATA_CLASS.into(),
    }
}

/// The seven Manager tools, always and regardless of model route.
pub fn manager_tool_descriptors() -> Vec<ToolDescriptor> {
    vec![
        manager_tool(
            PEOPLE_IDENTITY_READ,
            "Read the identities of the Person's selected contacts.",
            EMPTY_INPUT_SCHEMA,
        ),
        manager_tool(
            SCHEDULE_FEASIBILITY_READ,
            "Read whether the Person can still make a planned event.",
            EMPTY_INPUT_SCHEMA,
        ),
        manager_tool(
            ATTENTION_COARSE_READ,
            "Read the Person's coarse attention state.",
            EMPTY_INPUT_SCHEMA,
        ),
        manager_tool(
            WELLBEING_DERIVED_READ,
            "Read the Person's derived capacity today.",
            EMPTY_INPUT_SCHEMA,
        ),
        manager_tool(
            MAIL_COMMUNICATION_READ,
            "Search the Person's connected mail.",
            MAIL_INPUT_SCHEMA,
        ),
        manager_tool(
            WORK_CONTEXT_READ,
            "Read the Person's connected work context.",
            EMPTY_INPUT_SCHEMA,
        ),
        manager_tool(
            LIFE_LOGISTICS_READ,
            "Read the Person's connected logistics context.",
            EMPTY_INPUT_SCHEMA,
        ),
    ]
}

/// The Manager tools, bound to one Person and device.
///
/// Personal tools read through the Person's grants and device driver; remote
/// tools read through the injected remote source reader. A missing remote
/// reader means the remote sources are unavailable — never a reason to fall
/// back to another route.
pub struct ContextToolService<Records, Driver, Remote> {
    person_id: PersonId,
    device_id: String,
    records: Records,
    driver: Driver,
    remote: Option<Remote>,
}

impl<Records, Driver, Remote> ContextToolService<Records, Driver, Remote> {
    pub fn new(
        person_id: PersonId,
        device_id: impl Into<String>,
        records: Records,
        driver: Driver,
        remote: Option<Remote>,
    ) -> Result<Self, AgentFailure> {
        let device_id = device_id.into();
        if person_id.0.is_nil() || device_id.trim().is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            person_id,
            device_id,
            records,
            driver,
            remote,
        })
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MailInput {
    #[serde(default)]
    query: String,
    #[serde(default)]
    cursor: usize,
    #[serde(default = "default_communication_limit")]
    limit: usize,
}

impl<Records, Driver, Remote> ContextToolService<Records, Driver, Remote>
where
    Records: PersonalGrantRecords,
    Driver: PersonalSourceDriver,
    Remote: SourceReader,
{
    fn empty_input(call: &ToolCall) -> Result<(), AgentFailure> {
        serde_json::from_str::<EmptyInput>(&call.input).map_err(|_| AgentFailure::InvalidInput)?;
        Ok(())
    }

    fn mail_input(call: &ToolCall) -> Result<MailInput, AgentFailure> {
        let input: MailInput =
            serde_json::from_str(&call.input).map_err(|_| AgentFailure::InvalidInput)?;
        if input.query.chars().count() > MAX_MAIL_QUERY_CHARS
            || input.cursor > MAX_MAIL_CURSOR
            || input.limit == 0
            || input.limit > MAX_MAIL_LIMIT
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(input)
    }

    fn result(
        call: &ToolCall,
        payload: &serde_json::Value,
        dependency: floe_context_contract::ContextDependency,
    ) -> Result<ToolResult, AgentFailure> {
        Ok(ToolResult {
            call_id: call.call_id,
            text: serde_json::to_string(payload).map_err(|_| AgentFailure::InvalidInput)?,
            artifacts: vec![],
            coverage: DependencyCoverage::dependent(dependency)
                .map_err(|_| AgentFailure::InvalidInput)?,
            issue: None,
        })
    }

    async fn read_remote(
        &self,
        source_id: &str,
        query: serde_json::Value,
        scope: &floe_execution::ExecutionScope,
    ) -> Result<SourceView<serde_json::Value>, AgentFailure> {
        let Some(remote) = self.remote.as_ref() else {
            return Err(AgentFailure::CapabilityUnavailable);
        };
        let remote: &dyn SourceReader = remote;
        let service = ContextService::new(Some(remote));
        let prepared: PreparedContext<'_> = service.prepare(self.person_id)?;
        let request = prepared.source_request(
            source_id,
            GrantConsumer::builtin(ASSISTANT_CONSUMER)
                .map_err(|_| AgentFailure::InvalidInput)?,
            GrantPurpose::Assistant,
            query,
            scope.deadline(),
            scope.cancellation().clone(),
        )?;
        prepared.read_source(&request).await
    }
}

impl<Records, Driver, Remote> ToolPort for ContextToolService<Records, Driver, Remote>
where
    Records: PersonalGrantRecords,
    Driver: PersonalSourceDriver,
    Remote: SourceReader,
{
    fn invoke<'a>(
        &'a self,
        call: ToolCall,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        Box::pin(async move {
            if call.definition_revision != MANAGER_TOOL_DEFINITION_REVISION {
                return Err(AgentFailure::InvalidInput);
            }
            let deadline = scope.deadline();
            let cancellation = scope.cancellation().clone();
            match call.tool_id.as_str() {
                PEOPLE_IDENTITY_READ => {
                    Self::empty_input(&call)?;
                    let (view, dependency) = crate::read_manager_people(
                        &self.records,
                        &self.driver,
                        self.person_id,
                        &self.device_id,
                        ASSISTANT_CONSUMER,
                        deadline,
                        &cancellation,
                    )
                    .await?;
                    Self::result(
                        &call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )
                }
                SCHEDULE_FEASIBILITY_READ => {
                    Self::empty_input(&call)?;
                    let (view, dependency) = crate::read_feasibility(
                        &self.records,
                        &self.driver,
                        self.person_id,
                        &self.device_id,
                        ASSISTANT_CONSUMER,
                        call.call_id,
                        deadline,
                        &cancellation,
                    )
                    .await?;
                    Self::result(
                        &call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )
                }
                ATTENTION_COARSE_READ => {
                    Self::empty_input(&call)?;
                    let (view, dependency) = crate::admit_attention(
                        &self.records,
                        &self.driver,
                        self.person_id,
                        &self.device_id,
                        floe_access::attention_consumer(ASSISTANT_CONSUMER)?,
                        call.call_id,
                        deadline,
                        &cancellation,
                    )
                    .await?;
                    Self::result(
                        &call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )
                }
                WELLBEING_DERIVED_READ => {
                    Self::empty_input(&call)?;
                    let (view, dependency) = crate::read_wellbeing(
                        &self.records,
                        &self.driver,
                        self.person_id,
                        &self.device_id,
                        ASSISTANT_CONSUMER,
                        call.call_id,
                        deadline,
                        &cancellation,
                    )
                    .await?;
                    Self::result(
                        &call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )
                }
                MAIL_COMMUNICATION_READ => {
                    let input = Self::mail_input(&call)?;
                    let source_view = self
                        .read_remote(
                            "mail.communication",
                            serde_json::json!({
                                "schema_version": floe_agent_contract::AGENT_VERSION,
                                "query": input.query,
                                "cursor": input.cursor,
                                "limit": input.limit,
                            }),
                            scope,
                        )
                        .await?;
                    Self::result(&call, source_view.payload(), source_view.dependency().clone())
                }
                WORK_CONTEXT_READ => {
                    Self::empty_input(&call)?;
                    let source_view = self
                        .read_remote(
                            "work.context",
                            serde_json::json!({
                                "schema_version": floe_agent_contract::AGENT_VERSION,
                            }),
                            scope,
                        )
                        .await?;
                    Self::result(&call, source_view.payload(), source_view.dependency().clone())
                }
                LIFE_LOGISTICS_READ => {
                    Self::empty_input(&call)?;
                    let source_view = self
                        .read_remote(
                            "life.logistics",
                            serde_json::json!({
                                "schema_version": floe_agent_contract::AGENT_VERSION,
                            }),
                            scope,
                        )
                        .await?;
                    Self::result(&call, source_view.payload(), source_view.dependency().clone())
                }
                _ => Err(AgentFailure::CapabilityDenied),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use chrono::{Duration, Utc};
    use floe_access::{
        ConsumerPolicyAuthority, DataAccessGrant, FeasibilityGrantQuery, GrantDataCategory,
        GrantId, GrantOperation, GrantPurpose, GrantScope, ResourceHandle, SourceAuthority,
    };
    use floe_agent_contract::{
        AGENT_VERSION, AgentFailure, BoxFuture, InvocationKey, ToolCall,
    };
    use floe_context_contract::{
        ConnectionId, ConnectorId, ContextDependency, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantSourceBinding, ProcessingRestriction,
    };
    use floe_execution::{Cancellation, ExecutionScope};
    use tokio::time::Instant;
    use uuid::Uuid;

    use super::*;
    use crate::ports::personal_source::{
        AcquiredSource, AttentionAcquisition, PersonalAcquisition, PersonalDomain,
    };
    use crate::ports::source_reader::{SourceKey, SourceRead, SourceReadRequest};

    const DEVICE: &str = "device";
    const SUBJECT: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn scope_fixture(resource: &str) -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new(resource).unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("assistant").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    fn active_grant(source: floe_access::GrantSourceBinding, resource: &str) -> DataAccessGrant {
        let scope = scope_fixture(resource);
        let mut grant =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source.clone(), scope.clone())
                .unwrap();
        grant
            .activate_review(grant.authority(), source, scope)
            .unwrap();
        grant
    }

    struct FixtureRecords {
        grants: Vec<DataAccessGrant>,
        subject: String,
        handles: Vec<String>,
        query: FeasibilityGrantQuery,
    }

    impl FixtureRecords {
        fn new(person_id: PersonId) -> Self {
            let people = floe_access::contacts_source(
                person_id,
                DEVICE,
                "contacts.apple",
                SourceAuthority::new(),
            )
            .unwrap();
            let feasibility =
                floe_access::feasibility_source(person_id, DEVICE, SourceAuthority::new()).unwrap();
            let wellbeing =
                floe_access::wellbeing_source(person_id, DEVICE, SourceAuthority::new()).unwrap();
            let attention =
                floe_access::attention_source(person_id, DEVICE, SourceAuthority::new()).unwrap();
            Self {
                grants: vec![
                    active_grant(people, floe_access::PEOPLE_RESOURCE),
                    active_grant(feasibility, floe_access::FEASIBILITY_RESOURCE),
                    active_grant(wellbeing, floe_access::WELLBEING_RESOURCE),
                    active_grant(attention, floe_access::ATTENTION_RESOURCE),
                ],
                subject: SUBJECT.into(),
                handles: vec!["alice".into()],
                query: FeasibilityGrantQuery {
                    event_handle: "event:one".into(),
                    evidence_handles: vec!["calendar:one".into()],
                    destination_latitude: 37.5,
                    destination_longitude: 127.0,
                    event_start_unix_ms: 1_000,
                    event_end_unix_ms: 2_000,
                    travel_mode: "transit".into(),
                },
            }
        }
    }

    impl PersonalGrantRecords for FixtureRecords {
        fn grants<'a>(&'a self) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
            let grants = self.grants.clone();
            Box::pin(async move { Ok(grants) })
        }

        fn reviewed_subject<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<String, AgentFailure>> {
            let subject = self.subject.clone();
            Box::pin(async move { Ok(subject) })
        }

        fn consumer_policy<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<ConsumerPolicyAuthority, AgentFailure>> {
            Box::pin(async { Ok(ConsumerPolicyAuthority::default()) })
        }

        fn feasibility_query<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<FeasibilityGrantQuery, AgentFailure>> {
            let query = self.query.clone();
            Box::pin(async move { Ok(query) })
        }

        fn selected_handles<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<Vec<String>, AgentFailure>> {
            let handles = self.handles.clone();
            Box::pin(async move { Ok(handles) })
        }
    }

    fn people_view() -> serde_json::Value {
        let now = Utc::now().timestamp_millis();
        serde_json::json!({
            "schema_version": AGENT_VERSION,
            "view_id": "people.identity",
            "source_handle": "contacts:alice",
            "observed_at_unix_ms": now - 1_000,
            "expires_at_unix_ms": now + 60_000,
            "coverage_complete": true,
            "identities": [{
                "identity_handle": "alice",
                "display_name": "Alice",
                "aliases": [],
                "confidence_millis": 900,
                "evidence_handles": ["contacts:alice"],
            }],
        })
    }

    fn feasibility_view() -> serde_json::Value {
        let now = Utc::now().timestamp_millis();
        serde_json::json!({
            "schema_version": AGENT_VERSION,
            "view_id": "schedule.feasibility",
            "source_handle": "schedule:one",
            "observed_at_unix_ms": now - 1_000,
            "expires_at_unix_ms": now + 60_000,
            "items": [{
                "event_handle": "event:one",
                "evidence_handles": ["calendar:one"],
                "travel_duration_seconds": 600,
                "leave_by_unix_ms": now + 3_600_000,
                "weather_impact": "none",
                "confidence_millis": 900,
            }],
        })
    }

    fn attention_view() -> serde_json::Value {
        let now = Utc::now().timestamp_millis();
        serde_json::json!({
            "schema_version": AGENT_VERSION,
            "view_id": "attention.coarse",
            "source_handle": "device:camera",
            "observed_at_unix_ms": now - 1_000,
            "expires_at_unix_ms": now + 60_000,
            "state": "available",
            "confidence_millis": 900,
            "evidence_handles": ["device:camera"],
        })
    }

    fn wellbeing_view() -> serde_json::Value {
        let now = Utc::now().timestamp_millis();
        serde_json::json!({
            "schema_version": AGENT_VERSION,
            "view_id": "wellbeing.derived",
            "source_handle": "health:today",
            "observed_at_unix_ms": now - 1_000,
            "expires_at_unix_ms": now + 60_000,
            "capacity": "typical",
            "recovery": "typical",
            "confidence_millis": 900,
            "evidence_handles": ["health:today"],
        })
    }

    struct FixtureDriver {
        process: Uuid,
    }

    impl PersonalSourceDriver for FixtureDriver {
        fn personal_host_epoch(&self, _: PersonId) -> Result<String, AgentFailure> {
            Ok("host".into())
        }

        fn attention_host_epoch(&self, _: PersonId) -> Result<String, AgentFailure> {
            Ok("host".into())
        }

        fn process_incarnation(&self) -> Uuid {
            self.process
        }

        fn acquire<'a>(
            &'a self,
            request: PersonalAcquisition<'a>,
            _: Cancellation,
        ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
            let view = match request.domain {
                PersonalDomain::People => people_view(),
                PersonalDomain::Feasibility => feasibility_view(),
                PersonalDomain::Wellbeing => wellbeing_view(),
            };
            let subject = request.expected_subject.clone();
            Box::pin(async move {
                Ok(AcquiredSource {
                    view: Some(view),
                    subject_before: subject.clone(),
                    subject_after: subject,
                })
            })
        }

        fn acquire_attention<'a>(
            &'a self,
            request: AttentionAcquisition<'a>,
            _: Cancellation,
        ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
            let subject = request.expected_subject.clone().unwrap_or_default();
            Box::pin(async move {
                Ok(AcquiredSource {
                    view: Some(attention_view()),
                    subject_before: subject.clone(),
                    subject_after: subject,
                })
            })
        }

        fn commit_personal_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
            _: &str,
            _: i64,
            _: i64,
            _: Vec<u8>,
        ) -> Result<(), AgentFailure> {
            Ok(())
        }

        fn trusted_personal_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
        ) -> Result<crate::TrustedObservation, AgentFailure> {
            Err(AgentFailure::NotFound)
        }

        fn trusted_attention_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
        ) -> Result<(crate::AttentionView, String), AgentFailure> {
            Err(AgentFailure::NotFound)
        }

        fn commit_attention_projection(
            &self,
            _: PersonId,
            _: &str,
            _: &str,
            _: &crate::AttentionView,
            _: &str,
        ) -> Result<(Uuid, Uuid), AgentFailure> {
            Ok((Uuid::new_v4(), self.process))
        }
    }

    struct StaticRemote {
        payloads: Mutex<HashMap<String, serde_json::Value>>,
        seen: std::sync::Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl StaticRemote {
        fn new(payloads: Vec<(&str, serde_json::Value)>) -> Self {
            Self {
                payloads: Mutex::new(
                    payloads
                        .into_iter()
                        .map(|(source, payload)| (source.to_owned(), payload))
                        .collect(),
                ),
                seen: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn seen_handle(&self) -> std::sync::Arc<Mutex<Vec<(String, serde_json::Value)>>> {
            std::sync::Arc::clone(&self.seen)
        }
    }

    impl SourceReader for StaticRemote {
        fn read<'a>(
            &'a self,
            request: &'a SourceReadRequest,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<SourceRead, AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async move {
                let source = request.source().as_str().to_owned();
                self.seen
                    .lock()
                    .unwrap()
                    .push((source.clone(), request.query().clone()));
                let payload = self
                    .payloads
                    .lock()
                    .unwrap()
                    .get(&source)
                    .cloned()
                    .ok_or(AgentFailure::CapabilityUnavailable)?;
                let person_id = request.person_id();
                let source_binding = GrantSourceBinding::try_new(
                    person_id,
                    ConnectionId::try_new("connection").unwrap(),
                    ConnectorId::try_new("connector").unwrap(),
                    ExecutionOwnerId::try_new("owner").unwrap(),
                    SourceAuthority::new(),
                )
                .unwrap();
                let now = Utc::now();
                let consumer = request.consumer().clone();
                let processing = ProcessingRestriction::ApprovedRecipient {
                    recipient: "gateway-local".into(),
                    categories: vec![GrantDataCategory::Metadata],
                };
                let dependency = ContextDependency::try_new(
                    person_id,
                    GrantId::new(),
                    GrantAuthority::new(),
                    source_binding,
                    vec![ResourceHandle::try_new("resource").unwrap()],
                    vec![GrantDataCategory::Metadata],
                    GrantOperation::Read,
                    request.purpose(),
                    consumer.clone(),
                    processing.clone(),
                    ConsumerPolicyAuthority::new(),
                    Uuid::new_v4(),
                    request.query_fingerprint().to_vec(),
                    Uuid::new_v4(),
                    request.process_incarnation_id(),
                    now - Duration::minutes(1),
                    now + Duration::minutes(5),
                )
                .unwrap();
                let scope = GrantScope::try_new(
                    vec![ResourceHandle::try_new("resource").unwrap()],
                    vec![GrantDataCategory::Metadata],
                    vec![GrantOperation::Read],
                    vec![request.purpose()],
                    vec![consumer],
                    processing,
                )
                .unwrap();
                Ok(SourceRead::new(
                    SourceKey::try_new(source).unwrap(),
                    payload,
                    dependency,
                    scope,
                ))
            })
        }
    }

    fn service(
        person_id: PersonId,
        remote: Option<StaticRemote>,
    ) -> ContextToolService<FixtureRecords, FixtureDriver, StaticRemote> {
        ContextToolService::new(
            person_id,
            DEVICE,
            FixtureRecords::new(person_id),
            FixtureDriver {
                process: Uuid::new_v4(),
            },
            remote,
        )
        .unwrap()
    }

    fn scope() -> ExecutionScope {
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        ExecutionScope::root(
            Cancellation::default(),
            Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(Uuid::new_v4()),
        )
    }

    fn call(tool_id: &str, input: &str) -> ToolCall {
        ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: InvocationKey::new(),
            tool_id: tool_id.into(),
            definition_revision: MANAGER_TOOL_DEFINITION_REVISION,
            input: input.into(),
        }
    }

    fn dependent_of(result: &ToolResult) -> &ContextDependency {
        match &result.coverage {
            DependencyCoverage::Dependent { dependencies } => {
                assert_eq!(dependencies.len(), 1);
                &dependencies[0]
            }
            DependencyCoverage::Independent | DependencyCoverage::Unknown => {
                panic!("a source-backed result must be dependent")
            }
        }
    }

    #[test]
    fn catalog_contains_all_seven_tools_with_stable_canonical_shape() {
        let descriptors = manager_tool_descriptors();
        let ids: Vec<&str> = descriptors
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                PEOPLE_IDENTITY_READ,
                SCHEDULE_FEASIBILITY_READ,
                ATTENTION_COARSE_READ,
                WELLBEING_DERIVED_READ,
                MAIL_COMMUNICATION_READ,
                WORK_CONTEXT_READ,
                LIFE_LOGISTICS_READ,
            ]
        );
        for descriptor in &descriptors {
            descriptor.validate().unwrap();
            assert_eq!(
                descriptor.definition_revision,
                MANAGER_TOOL_DEFINITION_REVISION
            );
            assert_eq!(descriptor.output_data_class, "personal");
        }
        // The catalog is a pure function of nothing: availability cannot vary
        // with model route, profile, or recipient.
        let again = manager_tool_descriptors();
        assert_eq!(descriptors, again);
    }

    #[test]
    fn constructor_rejects_missing_identity() {
        assert_eq!(
            ContextToolService::new(
                PersonId::new(),
                "",
                FixtureRecords::new(PersonId::new()),
                FixtureDriver {
                    process: Uuid::new_v4()
                },
                None::<StaticRemote>,
            )
            .err(),
            Some(AgentFailure::InvalidInput)
        );
    }

    #[tokio::test]
    async fn unknown_tool_is_denied_and_wrong_revision_is_malformed() {
        let person_id = PersonId::new();
        let service = service(person_id, None);
        let scope = scope();
        assert_eq!(
            service.invoke(call("nope.read", "{}"), &scope).await.err(),
            Some(AgentFailure::CapabilityDenied)
        );
        let mut stale = call(PEOPLE_IDENTITY_READ, "{}");
        stale.definition_revision = MANAGER_TOOL_DEFINITION_REVISION + 1;
        assert_eq!(
            service.invoke(stale, &scope).await.err(),
            Some(AgentFailure::InvalidInput)
        );
    }

    #[tokio::test]
    async fn mail_input_bounds_remain_enforced() {
        let person_id = PersonId::new();
        let remote = StaticRemote::new(vec![("mail.communication", serde_json::json!({"hits": []}))]);
        let service = service(person_id, Some(remote));
        let scope = scope();
        for input in [
            r#"{"query": 1}"#,
            r#"{"cursor": -1}"#,
            r#"{"limit": 0}"#,
            r#"{"limit": 101}"#,
            r#"{"cursor": 10001}"#,
            r#"{"query": "ok", "unknown": true}"#,
            "not json",
        ] {
            assert_eq!(
                service
                    .invoke(call(MAIL_COMMUNICATION_READ, input), &scope)
                    .await
                    .err(),
                Some(AgentFailure::InvalidInput),
                "input must be rejected: {input}"
            );
        }
        let long = "q".repeat(513);
        assert_eq!(
            service
                .invoke(
                    call(MAIL_COMMUNICATION_READ, &format!(r#"{{"query": "{long}"}}"#)),
                    &scope,
                )
                .await
                .err(),
            Some(AgentFailure::InvalidInput)
        );
        for tool in [
            PEOPLE_IDENTITY_READ,
            SCHEDULE_FEASIBILITY_READ,
            ATTENTION_COARSE_READ,
            WELLBEING_DERIVED_READ,
            WORK_CONTEXT_READ,
            LIFE_LOGISTICS_READ,
        ] {
            assert_eq!(
                service.invoke(call(tool, r#"{"extra": true}"#), &scope).await.err(),
                Some(AgentFailure::InvalidInput),
                "{tool} takes an empty object only"
            );
        }
    }

    #[tokio::test]
    async fn local_tools_return_exact_dependent_coverage() {
        let person_id = PersonId::new();
        let service = service(person_id, None);
        let scope = scope();
        for (tool_id, marker) in [
            (PEOPLE_IDENTITY_READ, "people.identity"),
            (SCHEDULE_FEASIBILITY_READ, "schedule.feasibility"),
            (ATTENTION_COARSE_READ, "attention.coarse"),
            (WELLBEING_DERIVED_READ, "wellbeing.derived"),
        ] {
            let invocation = call(tool_id, "{}");
            let result = service.invoke(invocation.clone(), &scope).await.unwrap();
            assert_eq!(result.call_id, invocation.call_id);
            assert!(result.text.contains(marker), "{tool_id}: {}", result.text);
            assert!(result.artifacts.is_empty());
            assert!(result.issue.is_none());
            let dependency = dependent_of(&result);
            assert_eq!(dependency.person_id(), person_id);
            assert_eq!(dependency.operation(), GrantOperation::Read);
            result.validate(invocation.call_id, 32_768).unwrap();
        }
    }

    #[tokio::test]
    async fn remote_tools_return_exact_dependent_coverage() {
        let person_id = PersonId::new();
        let remote = StaticRemote::new(vec![
            ("mail.communication", serde_json::json!({"hits": ["m1"]})),
            ("work.context", serde_json::json!({"projects": []})),
            ("life.logistics", serde_json::json!({"shipments": []})),
        ]);
        let service = service(person_id, Some(remote));
        let scope = scope();
        let mail = service
            .invoke(
                call(MAIL_COMMUNICATION_READ, r#"{"query": "invoice", "limit": 5}"#),
                &scope,
            )
            .await
            .unwrap();
        assert!(mail.text.contains("m1"));
        let mail_dep = dependent_of(&mail);
        assert_eq!(mail_dep.person_id(), person_id);
        assert!(matches!(
            mail_dep.processing(),
            ProcessingRestriction::ApprovedRecipient { .. }
        ));
        assert!(mail.artifacts.is_empty());
        for (tool_id, marker) in [
            (WORK_CONTEXT_READ, "projects"),
            (LIFE_LOGISTICS_READ, "shipments"),
        ] {
            let invocation = call(tool_id, "{}");
            let result = service.invoke(invocation.clone(), &scope).await.unwrap();
            assert!(result.text.contains(marker));
            let dependency = dependent_of(&result);
            assert_eq!(dependency.person_id(), person_id);
            result.validate(invocation.call_id, 32_768).unwrap();
        }
    }

    #[tokio::test]
    async fn mail_defaults_apply_when_fields_are_absent() {
        let person_id = PersonId::new();
        let remote = StaticRemote::new(vec![("mail.communication", serde_json::json!({"hits": []}))]);
        let seen = remote.seen_handle();
        let service = ContextToolService::new(
            person_id,
            DEVICE,
            FixtureRecords::new(person_id),
            FixtureDriver {
                process: Uuid::new_v4(),
            },
            Some(remote),
        )
        .unwrap();
        let scope = scope();
        service
            .invoke(call(MAIL_COMMUNICATION_READ, "{}"), &scope)
            .await
            .unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "mail.communication");
        assert_eq!(seen[0].1["query"], serde_json::json!(""));
        assert_eq!(seen[0].1["cursor"], serde_json::json!(0));
        assert_eq!(seen[0].1["limit"], serde_json::json!(25));
    }

    #[tokio::test]
    async fn missing_remote_source_is_unavailable_not_a_fallback() {
        let person_id = PersonId::new();
        let service = service(person_id, None);
        let scope = scope();
        for tool_id in [MAIL_COMMUNICATION_READ, WORK_CONTEXT_READ, LIFE_LOGISTICS_READ] {
            let input = if tool_id == MAIL_COMMUNICATION_READ {
                r#"{"query": "x"}"#
            } else {
                "{}"
            };
            assert_eq!(
                service.invoke(call(tool_id, input), &scope).await.err(),
                Some(AgentFailure::CapabilityUnavailable),
                "{tool_id} without a remote reader"
            );
        }
        // Local tools are unaffected by the missing remote reader.
        service
            .invoke(call(PEOPLE_IDENTITY_READ, "{}"), &scope)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn revoked_people_selection_requires_review() {
        let person_id = PersonId::new();
        let mut records = FixtureRecords::new(person_id);
        records.handles.clear();
        let service = ContextToolService::new(
            person_id,
            DEVICE,
            records,
            FixtureDriver {
                process: Uuid::new_v4(),
            },
            None::<StaticRemote>,
        )
        .unwrap();
        let scope = scope();
        assert_eq!(
            service
                .invoke(call(PEOPLE_IDENTITY_READ, "{}"), &scope)
                .await
                .err(),
            Some(AgentFailure::AccessReviewRequired)
        );
    }
}
