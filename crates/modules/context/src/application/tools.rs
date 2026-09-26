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
    AgentFailure, DependencyCoverage, PersonId, ToolCall, ToolDescriptor, ToolResult,
};
use floe_context_contract::{
    GrantConsumer, GrantOperation, GrantPurpose, SourceAccessBlockers, SourceAccessRequirement,
    SourceAccessRequirementKind, SourceReadOutcome,
};

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

struct ManagerToolSpec {
    id: &'static str,
    description: &'static str,
    input_schema: &'static str,
    remote_view: Option<&'static str>,
}

const MANAGER_TOOLS: [ManagerToolSpec; 7] = [
    ManagerToolSpec {
        id: PEOPLE_IDENTITY_READ,
        description: "Read the identities of the Person's selected contacts.",
        input_schema: EMPTY_INPUT_SCHEMA,
        remote_view: None,
    },
    ManagerToolSpec {
        id: SCHEDULE_FEASIBILITY_READ,
        description: "Read whether the Person can still make a planned event.",
        input_schema: EMPTY_INPUT_SCHEMA,
        remote_view: None,
    },
    ManagerToolSpec {
        id: ATTENTION_COARSE_READ,
        description: "Read the Person's coarse attention state.",
        input_schema: EMPTY_INPUT_SCHEMA,
        remote_view: None,
    },
    ManagerToolSpec {
        id: WELLBEING_DERIVED_READ,
        description: "Read the Person's derived capacity today.",
        input_schema: EMPTY_INPUT_SCHEMA,
        remote_view: None,
    },
    ManagerToolSpec {
        id: MAIL_COMMUNICATION_READ,
        description: "Search the Person's connected mail.",
        input_schema: MAIL_INPUT_SCHEMA,
        remote_view: Some(crate::MAIL_VIEW),
    },
    ManagerToolSpec {
        id: WORK_CONTEXT_READ,
        description: "Read the Person's connected work context.",
        input_schema: EMPTY_INPUT_SCHEMA,
        remote_view: Some(crate::WORK_VIEW),
    },
    ManagerToolSpec {
        id: LIFE_LOGISTICS_READ,
        description: "Read the Person's connected logistics context.",
        input_schema: EMPTY_INPUT_SCHEMA,
        remote_view: Some(crate::LOGISTICS_VIEW),
    },
];

pub fn manager_direct_remote_view(view_id: &str) -> bool {
    MANAGER_TOOLS
        .iter()
        .any(|tool| tool.remote_view == Some(view_id))
}

pub fn manager_direct_native_connector(connector_id: &str) -> bool {
    let tool_id = match connector_id {
        "attention.macos" => ATTENTION_COARSE_READ,
        "contacts.apple" | "contacts.android" => PEOPLE_IDENTITY_READ,
        "health.apple" => WELLBEING_DERIVED_READ,
        "feasibility.apple" => SCHEDULE_FEASIBILITY_READ,
        _ => return false,
    };
    MANAGER_TOOLS.iter().any(|tool| tool.id == tool_id)
}

/// The seven Manager tools, always and regardless of model route.
pub fn manager_tool_descriptors() -> Vec<ToolDescriptor> {
    MANAGER_TOOLS
        .iter()
        .map(|tool| manager_tool(tool.id, tool.description, tool.input_schema))
        .collect()
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

    fn result_bound(
        call: &ToolCall,
        payload: &serde_json::Value,
        bindings: &[floe_context_contract::AuthorizedSourceBinding],
    ) -> Result<ToolResult, AgentFailure> {
        let mut coverage = DependencyCoverage::Independent;
        for binding in bindings {
            coverage = coverage
                .merge(
                    &DependencyCoverage::dependent(binding.dependency.clone())
                        .map_err(|_| AgentFailure::InvalidInput)?,
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
        }
        Ok(ToolResult {
            call_id: call.call_id,
            text: serde_json::to_string(payload).map_err(|_| AgentFailure::InvalidInput)?,
            artifacts: vec![],
            coverage,
            issue: None,
        })
    }

    async fn read_remote(
        &self,
        source_id: &str,
        query: serde_json::Value,
        scope: &floe_execution::ExecutionScope,
    ) -> Result<SourceReadOutcome<SourceView<serde_json::Value>>, AgentFailure> {
        let Some(remote) = self.remote.as_ref() else {
            // No remote reader means no configured remote source: one
            // navigation-only requirement for the source category, inventing
            // no connection, resource or fingerprint.
            let category = match source_id {
                "mail.communication" => "floe.source.mail",
                "work.context" => "floe.source.work-context",
                "life.logistics" => "floe.source.logistics",
                _ => return Err(AgentFailure::InvalidInput),
            };
            let requirement = SourceAccessRequirement::try_new(
                category,
                None,
                None,
                GrantOperation::Read,
                GrantConsumer::builtin(ASSISTANT_CONSUMER)
                    .map_err(|_| AgentFailure::InvalidInput)?,
                GrantPurpose::Assistant,
                vec![],
                None,
                SourceAccessRequirementKind::SelectResource,
                None,
                None,
                false,
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
            let blockers = SourceAccessBlockers::try_new(vec![requirement])
                .map_err(|_| AgentFailure::InvalidInput)?;
            return Ok(SourceReadOutcome::NeedsUserAction(blockers));
        };
        let remote: &dyn SourceReader = remote;
        let service = ContextService::new(Some(remote));
        let prepared: PreparedContext<'_> = service.prepare(self.person_id)?;
        let request = prepared.source_request(
            source_id,
            GrantConsumer::builtin(ASSISTANT_CONSUMER).map_err(|_| AgentFailure::InvalidInput)?,
            GrantPurpose::Assistant,
            query,
            scope.deadline(),
            scope.cancellation().clone(),
        )?;
        prepared.read_source(&request).await
    }

    /// Invoke one tool, preserving a recoverable source blocker as a typed
    /// outcome instead of raising it. Only hard failures raise.
    pub async fn invoke_outcome(
        &self,
        call: &ToolCall,
        scope: &floe_execution::ExecutionScope,
    ) -> Result<SourceReadOutcome<ToolResult>, AgentFailure> {
        if call.definition_revision != MANAGER_TOOL_DEFINITION_REVISION {
            return Err(AgentFailure::InvalidInput);
        }
        let deadline = scope.deadline();
        let cancellation = scope.cancellation().clone();
        match call.tool_id.as_str() {
            PEOPLE_IDENTITY_READ => {
                Self::empty_input(call)?;
                let consumer = GrantConsumer::builtin(ASSISTANT_CONSUMER)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                match crate::read_manager_people_outcome(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    ASSISTANT_CONSUMER,
                    consumer,
                    deadline,
                    &cancellation,
                )
                .await?
                {
                    SourceReadOutcome::Ready((view, dependency)) => {
                        Ok(SourceReadOutcome::Ready(Self::result(
                            call,
                            &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                            dependency,
                        )?))
                    }
                    SourceReadOutcome::Unavailable(reason) => {
                        Ok(SourceReadOutcome::Unavailable(reason))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => {
                        Ok(SourceReadOutcome::NeedsUserAction(blockers))
                    }
                }
            }
            SCHEDULE_FEASIBILITY_READ => {
                Self::empty_input(call)?;
                let consumer = GrantConsumer::builtin(ASSISTANT_CONSUMER)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                match crate::read_feasibility_outcome(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    ASSISTANT_CONSUMER,
                    consumer,
                    call.call_id,
                    deadline,
                    &cancellation,
                )
                .await?
                {
                    SourceReadOutcome::Ready((view, dependency)) => {
                        Ok(SourceReadOutcome::Ready(Self::result(
                            call,
                            &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                            dependency,
                        )?))
                    }
                    SourceReadOutcome::Unavailable(reason) => {
                        Ok(SourceReadOutcome::Unavailable(reason))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => {
                        Ok(SourceReadOutcome::NeedsUserAction(blockers))
                    }
                }
            }
            ATTENTION_COARSE_READ => {
                Self::empty_input(call)?;
                let consumer = floe_access::attention_consumer(ASSISTANT_CONSUMER)?;
                match crate::admit_attention_outcome(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    consumer,
                    call.call_id,
                    deadline,
                    &cancellation,
                )
                .await?
                {
                    SourceReadOutcome::Ready((view, dependency)) => {
                        Ok(SourceReadOutcome::Ready(Self::result(
                            call,
                            &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                            dependency,
                        )?))
                    }
                    SourceReadOutcome::Unavailable(reason) => {
                        Ok(SourceReadOutcome::Unavailable(reason))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => {
                        Ok(SourceReadOutcome::NeedsUserAction(blockers))
                    }
                }
            }
            WELLBEING_DERIVED_READ => {
                Self::empty_input(call)?;
                let consumer = GrantConsumer::builtin(ASSISTANT_CONSUMER)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                match crate::read_wellbeing_outcome(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    ASSISTANT_CONSUMER,
                    consumer,
                    call.call_id,
                    deadline,
                    &cancellation,
                )
                .await?
                {
                    SourceReadOutcome::Ready((view, dependency)) => {
                        Ok(SourceReadOutcome::Ready(Self::result(
                            call,
                            &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                            dependency,
                        )?))
                    }
                    SourceReadOutcome::Unavailable(reason) => {
                        Ok(SourceReadOutcome::Unavailable(reason))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => {
                        Ok(SourceReadOutcome::NeedsUserAction(blockers))
                    }
                }
            }
            MAIL_COMMUNICATION_READ => {
                let input = Self::mail_input(call)?;
                match self
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
                    .await?
                {
                    SourceReadOutcome::Ready(view) => Ok(SourceReadOutcome::Ready(
                        Self::result_bound(call, view.payload(), view.bindings())?,
                    )),
                    SourceReadOutcome::Unavailable(reason) => {
                        Ok(SourceReadOutcome::Unavailable(reason))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => {
                        Ok(SourceReadOutcome::NeedsUserAction(blockers))
                    }
                }
            }
            WORK_CONTEXT_READ => {
                Self::empty_input(call)?;
                match self
                    .read_remote(
                        "work.context",
                        serde_json::json!({
                            "schema_version": floe_agent_contract::AGENT_VERSION,
                        }),
                        scope,
                    )
                    .await?
                {
                    SourceReadOutcome::Ready(view) => Ok(SourceReadOutcome::Ready(
                        Self::result_bound(call, view.payload(), view.bindings())?,
                    )),
                    SourceReadOutcome::Unavailable(reason) => {
                        Ok(SourceReadOutcome::Unavailable(reason))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => {
                        Ok(SourceReadOutcome::NeedsUserAction(blockers))
                    }
                }
            }
            LIFE_LOGISTICS_READ => {
                Self::empty_input(call)?;
                match self
                    .read_remote(
                        "life.logistics",
                        serde_json::json!({
                            "schema_version": floe_agent_contract::AGENT_VERSION,
                        }),
                        scope,
                    )
                    .await?
                {
                    SourceReadOutcome::Ready(view) => Ok(SourceReadOutcome::Ready(
                        Self::result_bound(call, view.payload(), view.bindings())?,
                    )),
                    SourceReadOutcome::Unavailable(reason) => {
                        Ok(SourceReadOutcome::Unavailable(reason))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => {
                        Ok(SourceReadOutcome::NeedsUserAction(blockers))
                    }
                }
            }
            _ => Err(AgentFailure::CapabilityDenied),
        }
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
    use floe_agent_contract::{AGENT_VERSION, AgentFailure, BoxFuture, InvocationKey, ToolCall};
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
    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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
        let mut grant = DataAccessGrant::new(
            GrantId::new(),
            Uuid::new_v4(),
            source.clone(),
            scope.clone(),
        )
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
            Box<
                dyn std::future::Future<
                        Output = Result<
                            floe_context_contract::SourceReadOutcome<SourceRead>,
                            AgentFailure,
                        >,
                    > + Send
                    + 'a,
            >,
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
                Ok(floe_context_contract::SourceReadOutcome::Ready(
                    SourceRead::new(
                        SourceKey::try_new(source).unwrap(),
                        payload,
                        dependency,
                        scope,
                    ),
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

    async fn ready(
        service: &ContextToolService<FixtureRecords, FixtureDriver, StaticRemote>,
        call: &ToolCall,
        scope: &ExecutionScope,
    ) -> ToolResult {
        let outcome = service.invoke_outcome(call, scope).await.unwrap();
        let floe_context_contract::SourceReadOutcome::Ready(result) = outcome else {
            panic!("admitted read must stay ready");
        };
        result
    }

    #[test]
    fn catalog_contains_all_seven_tools_with_stable_canonical_shape() {
        for connector in [
            "attention.macos",
            "contacts.apple",
            "contacts.android",
            "health.apple",
            "feasibility.apple",
        ] {
            assert!(manager_direct_native_connector(connector));
        }
        assert!(!manager_direct_native_connector("example.test.connector"));
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
            service
                .invoke_outcome(&call("nope.read", "{}"), &scope)
                .await
                .err(),
            Some(AgentFailure::CapabilityDenied)
        );
        let mut stale = call(PEOPLE_IDENTITY_READ, "{}");
        stale.definition_revision = MANAGER_TOOL_DEFINITION_REVISION + 1;
        assert_eq!(
            service.invoke_outcome(&stale, &scope).await.err(),
            Some(AgentFailure::InvalidInput)
        );
    }

    #[tokio::test]
    async fn mail_input_bounds_remain_enforced() {
        let person_id = PersonId::new();
        let remote = StaticRemote::new(vec![(
            "mail.communication",
            serde_json::json!({"hits": []}),
        )]);
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
                    .invoke_outcome(&call(MAIL_COMMUNICATION_READ, input), &scope)
                    .await
                    .err(),
                Some(AgentFailure::InvalidInput),
                "input must be rejected: {input}"
            );
        }
        let long = "q".repeat(513);
        assert_eq!(
            service
                .invoke_outcome(
                    &call(
                        MAIL_COMMUNICATION_READ,
                        &format!(r#"{{"query": "{long}"}}"#)
                    ),
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
                service
                    .invoke_outcome(&call(tool, r#"{"extra": true}"#), &scope)
                    .await
                    .err(),
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
            let result = ready(&service, &invocation, &scope).await;
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
        let mail = ready(
            &service,
            &call(
                MAIL_COMMUNICATION_READ,
                r#"{"query": "invoice", "limit": 5}"#,
            ),
            &scope,
        )
        .await;
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
            let result = ready(&service, &invocation, &scope).await;
            assert!(result.text.contains(marker));
            let dependency = dependent_of(&result);
            assert_eq!(dependency.person_id(), person_id);
            result.validate(invocation.call_id, 32_768).unwrap();
        }
    }

    #[tokio::test]
    async fn mail_defaults_apply_when_fields_are_absent() {
        let person_id = PersonId::new();
        let remote = StaticRemote::new(vec![(
            "mail.communication",
            serde_json::json!({"hits": []}),
        )]);
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
        ready(&service, &call(MAIL_COMMUNICATION_READ, "{}"), &scope).await;
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "mail.communication");
        assert_eq!(seen[0].1["query"], serde_json::json!(""));
        assert_eq!(seen[0].1["cursor"], serde_json::json!(0));
        assert_eq!(seen[0].1["limit"], serde_json::json!(25));
    }

    fn blocked_requirement(
        outcome: floe_context_contract::SourceReadOutcome<ToolResult>,
    ) -> floe_context_contract::SourceAccessRequirement {
        let floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("blocked read must keep its typed requirement");
        };
        blockers.validate().unwrap();
        assert_eq!(blockers.blockers().len(), 1);
        blockers.blockers()[0].clone()
    }

    #[tokio::test]
    async fn missing_remote_source_is_a_navigation_only_requirement() {
        let person_id = PersonId::new();
        let service = service(person_id, None);
        let scope = scope();
        for (tool_id, category) in [
            (MAIL_COMMUNICATION_READ, "floe.source.mail"),
            (WORK_CONTEXT_READ, "floe.source.work-context"),
            (LIFE_LOGISTICS_READ, "floe.source.logistics"),
        ] {
            let input = if tool_id == MAIL_COMMUNICATION_READ {
                r#"{"query": "x"}"#
            } else {
                "{}"
            };
            let outcome = service
                .invoke_outcome(&call(tool_id, input), &scope)
                .await
                .unwrap();
            let requirement = blocked_requirement(outcome);
            assert_eq!(requirement.source_id(), category, "{tool_id}");
            assert_eq!(
                requirement.reason(),
                floe_context_contract::SourceAccessRequirementKind::SelectResource
            );
            assert!(requirement.connection_id().is_none());
            assert!(requirement.connector_id().is_none());
            assert!(requirement.resources().is_empty());
            assert!(!requirement.inline_resolution());
        }
        // Local tools are unaffected by the missing remote reader.
        let outcome = service
            .invoke_outcome(&call(PEOPLE_IDENTITY_READ, "{}"), &scope)
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            floe_context_contract::SourceReadOutcome::Ready(_)
        ));
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
        let outcome = service
            .invoke_outcome(&call(PEOPLE_IDENTITY_READ, "{}"), &scope)
            .await
            .unwrap();
        let requirement = blocked_requirement(outcome);
        assert_eq!(
            requirement.reason(),
            floe_context_contract::SourceAccessRequirementKind::SelectResource
        );
        // The granting connection is known, but selection happens in the
        // picker, never inline.
        assert!(requirement.connection_id().is_some());
        assert!(!requirement.inline_resolution());
    }

    #[tokio::test]
    async fn missing_personal_grant_is_reenableable_without_invented_identity() {
        let person_id = PersonId::new();
        let mut records = FixtureRecords::new(person_id);
        records.grants.clear();
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
        // Fixed device-local identity: inline enable with proven absence.
        for (tool_id, source_id) in [
            (ATTENTION_COARSE_READ, "floe.source.attention"),
            (WELLBEING_DERIVED_READ, "floe.source.wellbeing"),
            (SCHEDULE_FEASIBILITY_READ, "floe.source.feasibility"),
        ] {
            let outcome = service
                .invoke_outcome(&call(tool_id, "{}"), &scope)
                .await
                .unwrap();
            let requirement = blocked_requirement(outcome);
            assert_eq!(requirement.source_id(), source_id);
            assert_eq!(
                requirement.reason(),
                floe_context_contract::SourceAccessRequirementKind::EnableObserve
            );
            assert_eq!(requirement.observed_grant(), None);
            assert!(requirement.connection_id().is_some());
            assert!(requirement.inline_resolution());
        }
        // Contacts span two platform connectors: no grant names one, so no
        // connection is invented and the review navigates to settings.
        let outcome = service
            .invoke_outcome(&call(PEOPLE_IDENTITY_READ, "{}"), &scope)
            .await
            .unwrap();
        let requirement = blocked_requirement(outcome);
        assert_eq!(requirement.source_id(), "floe.source.contacts");
        assert_eq!(
            requirement.reason(),
            floe_context_contract::SourceAccessRequirementKind::EnableObserve
        );
        assert!(requirement.connection_id().is_none());
        assert!(!requirement.inline_resolution());
    }

    #[tokio::test]
    async fn paused_personal_grant_binds_the_observed_grant() {
        let person_id = PersonId::new();
        let source =
            floe_access::attention_source(person_id, DEVICE, SourceAuthority::new()).unwrap();
        let scope_fixture = scope_fixture(floe_access::ATTENTION_RESOURCE);
        let paused =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source, scope_fixture).unwrap();
        assert_eq!(paused.state(), floe_access::GrantState::Paused);
        let mut records = FixtureRecords::new(person_id);
        records.grants = vec![paused.clone()];
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
        let outcome = service
            .invoke_outcome(&call(ATTENTION_COARSE_READ, "{}"), &scope)
            .await
            .unwrap();
        let requirement = blocked_requirement(outcome);
        assert_eq!(
            requirement.reason(),
            floe_context_contract::SourceAccessRequirementKind::EnableObserve
        );
        let observed = requirement.observed_grant().unwrap();
        assert_eq!(observed.grant_id(), paused.id());
        assert!(requirement.inline_resolution());
    }

    #[tokio::test]
    async fn duplicate_and_corrupt_personal_authority_fail_closed() {
        let person_id = PersonId::new();
        let source =
            floe_access::attention_source(person_id, DEVICE, SourceAuthority::new()).unwrap();
        let grants = vec![
            DataAccessGrant::new(
                GrantId::new(),
                Uuid::new_v4(),
                source.clone(),
                scope_fixture(floe_access::ATTENTION_RESOURCE),
            )
            .unwrap(),
            DataAccessGrant::new(
                GrantId::new(),
                Uuid::new_v4(),
                source,
                scope_fixture(floe_access::ATTENTION_RESOURCE),
            )
            .unwrap(),
        ];
        let mut records = FixtureRecords::new(person_id);
        records.grants = grants;
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
                .invoke_outcome(&call(ATTENTION_COARSE_READ, "{}"), &scope)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );

        struct CorruptRecords;
        impl PersonalGrantRecords for CorruptRecords {
            fn grants<'a>(&'a self) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
                Box::pin(async { Err(AgentFailure::StorageUnavailable) })
            }

            fn reviewed_subject<'a>(
                &'a self,
                _: GrantId,
            ) -> BoxFuture<'a, Result<String, AgentFailure>> {
                Box::pin(async { Err(AgentFailure::StorageUnavailable) })
            }

            fn consumer_policy<'a>(
                &'a self,
                _: GrantId,
            ) -> BoxFuture<'a, Result<ConsumerPolicyAuthority, AgentFailure>> {
                Box::pin(async { Err(AgentFailure::StorageUnavailable) })
            }

            fn selected_handles<'a>(
                &'a self,
                _: GrantId,
            ) -> BoxFuture<'a, Result<Vec<String>, AgentFailure>> {
                Box::pin(async { Err(AgentFailure::StorageUnavailable) })
            }

            fn feasibility_query<'a>(
                &'a self,
                _: GrantId,
            ) -> BoxFuture<'a, Result<FeasibilityGrantQuery, AgentFailure>> {
                Box::pin(async { Err(AgentFailure::StorageUnavailable) })
            }
        }
        let service = ContextToolService::new(
            person_id,
            DEVICE,
            CorruptRecords,
            FixtureDriver {
                process: Uuid::new_v4(),
            },
            None::<StaticRemote>,
        )
        .unwrap();
        assert_eq!(
            service
                .invoke_outcome(&call(ATTENTION_COARSE_READ, "{}"), &scope)
                .await
                .err(),
            Some(AgentFailure::StorageUnavailable)
        );
    }
}
