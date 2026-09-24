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
use floe_context_contract::{
    GrantConsumer, GrantOperation, GrantPurpose, GrantSourceBinding, ObservedGrant,
    ResourceHandle, SourceAccessBlockers, SourceAccessRequirement, SourceAccessRequirementKind,
    SourceAuthority, SourceReadOutcome, SourceUnavailable,
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

    /// The owner-known identity of one personal source a direct tool reads.
    fn personal_identity(
        &self,
        tool_id: &str,
    ) -> Result<PersonalSourceIdentity, AgentFailure> {
        let authority = SourceAuthority::new();
        match tool_id {
            PEOPLE_IDENTITY_READ => {
                let mut expected = Vec::with_capacity(2);
                for connector in ["contacts.apple", "contacts.android"] {
                    expected.push(
                        GrantSourceBinding::try_new(
                            self.person_id,
                            floe_context_contract::ConnectionId::try_new(
                                floe_access::contacts_connection(connector),
                            )
                            .map_err(|_| AgentFailure::InvalidInput)?,
                            floe_context_contract::ConnectorId::try_new(connector)
                                .map_err(|_| AgentFailure::InvalidInput)?,
                            floe_context_contract::ExecutionOwnerId::try_new(
                                floe_access::contacts_execution_owner(connector, &self.device_id),
                            )
                            .map_err(|_| AgentFailure::InvalidInput)?,
                            authority,
                        )
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    );
                }
                Ok(PersonalSourceIdentity {
                    source_id: "floe.source.contacts",
                    resource: floe_access::PEOPLE_RESOURCE,
                    expected,
                    // The platform connector is unknown until a grant names
                    // it: a missing grant cannot offer inline enable.
                    known_identity: None,
                })
            }
            SCHEDULE_FEASIBILITY_READ => Ok(PersonalSourceIdentity {
                source_id: "floe.source.feasibility",
                resource: floe_access::FEASIBILITY_RESOURCE,
                expected: vec![floe_access::feasibility_source(
                    self.person_id,
                    &self.device_id,
                    authority,
                )?],
                known_identity: Some((
                    floe_access::FEASIBILITY_CONNECTOR,
                    floe_access::FEASIBILITY_CONNECTION,
                )),
            }),
            ATTENTION_COARSE_READ => Ok(PersonalSourceIdentity {
                source_id: "floe.source.attention",
                resource: floe_access::ATTENTION_RESOURCE,
                expected: vec![floe_access::attention_source(
                    self.person_id,
                    &self.device_id,
                    authority,
                )?],
                known_identity: Some((
                    floe_access::ATTENTION_CONNECTOR,
                    floe_access::ATTENTION_CONNECTION,
                )),
            }),
            WELLBEING_DERIVED_READ => Ok(PersonalSourceIdentity {
                source_id: "floe.source.wellbeing",
                resource: floe_access::WELLBEING_RESOURCE,
                expected: vec![floe_access::wellbeing_source(
                    self.person_id,
                    &self.device_id,
                    authority,
                )?],
                known_identity: Some((
                    floe_access::WELLBEING_CONNECTOR,
                    floe_access::WELLBEING_CONNECTION,
                )),
            }),
            _ => Err(AgentFailure::InvalidInput),
        }
    }

    fn personal_requirement(
        identity: &PersonalSourceIdentity,
        connector: Option<floe_context_contract::ConnectorId>,
        connection: Option<floe_context_contract::ConnectionId>,
        consumer: GrantConsumer,
        reason: SourceAccessRequirementKind,
        authority: Option<SourceAuthority>,
        observed: Option<ObservedGrant>,
    ) -> Result<SourceAccessRequirement, AgentFailure> {
        let resources = if connector.is_some() && connection.is_some() {
            vec![
                ResourceHandle::try_new(identity.resource)
                    .map_err(|_| AgentFailure::InvalidInput)?,
            ]
        } else {
            vec![]
        };
        let inline = !resources.is_empty()
            && matches!(
                reason,
                SourceAccessRequirementKind::EnableObserve
                    | SourceAccessRequirementKind::ReviewChangedSource
            );
        SourceAccessRequirement::try_new(
            identity.source_id,
            connector,
            connection,
            GrantOperation::Read,
            consumer,
            GrantPurpose::Assistant,
            resources,
            None,
            reason,
            authority,
            observed,
            inline,
        )
        .map_err(|_| AgentFailure::InvalidInput)
    }

    /// The live grants binding this personal source, if the review can name
    /// exactly one. Duplicates fail closed; absence is proven absence.
    fn observe_personal_binding(
        grants: &[floe_access::DataAccessGrant],
        identity: &PersonalSourceIdentity,
    ) -> Result<Option<floe_access::DataAccessGrant>, AgentFailure> {
        let mut binding = grants.iter().filter(|grant| {
            grant.state() != floe_access::GrantState::Revoked
                && identity
                    .expected
                    .iter()
                    .any(|expected| grant.source().same_identity(expected))
        });
        let Some(grant) = binding.next() else {
            return Ok(None);
        };
        if binding.next().is_some() {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(Some(grant.clone()))
    }

    fn observed_grant(
        grant: &floe_access::DataAccessGrant,
    ) -> Result<ObservedGrant, AgentFailure> {
        ObservedGrant::try_new(grant.id(), grant.authority())
            .map_err(|_| AgentFailure::InvalidInput)
    }

    /// Classify a failed personal read against current grant facts.
    ///
    /// Missing and paused grants are re-enableable, drift and vanished grants
    /// need review, OS denial names system permission, and duplicate authority
    /// fails closed. Budget, cancellation, deadline, invalid input and corrupt
    /// storage stay hard failures with no card.
    async fn classify_personal_blocker(
        &self,
        identity: &PersonalSourceIdentity,
        consumer: GrantConsumer,
        error: AgentFailure,
    ) -> Result<PersonalBlock, AgentFailure> {
        match error {
            AgentFailure::CapabilityUnavailable => {
                return Ok(PersonalBlock::Unavailable(
                    SourceUnavailable::TemporarilyUnavailable,
                ));
            }
            AgentFailure::AccessReviewRequired
            | AgentFailure::CredentialExpired
            | AgentFailure::CapabilityDenied
            | AgentFailure::PolicyDenied => {}
            _ => return Err(error),
        }
        let grants = self.records.grants().await?;
        let binding = Self::observe_personal_binding(&grants, identity)?;
        let known = |identity: &PersonalSourceIdentity| {
            identity.known_identity.and_then(|(connector, connection)| {
                Some((
                    floe_context_contract::ConnectorId::try_new(connector).ok()?,
                    floe_context_contract::ConnectionId::try_new(connection).ok()?,
                ))
            })
        };
        let requirement = match error {
            AgentFailure::AccessReviewRequired => match &binding {
                None => {
                    let (connector, connection) =
                        known(identity).unzip();
                    Self::personal_requirement(
                        identity,
                        connector,
                        connection,
                        consumer,
                        SourceAccessRequirementKind::EnableObserve,
                        None,
                        None,
                    )?
                }
                Some(grant) => {
                    let reason = if grant.state() == floe_access::GrantState::Paused {
                        SourceAccessRequirementKind::EnableObserve
                    } else {
                        SourceAccessRequirementKind::ReviewChangedSource
                    };
                    Self::personal_requirement(
                        identity,
                        Some(grant.source().connector().clone()),
                        Some(grant.source().connection_id()),
                        consumer,
                        reason,
                        Some(grant.source().source_authority()),
                        Some(Self::observed_grant(grant)?),
                    )?
                }
            },
            AgentFailure::CredentialExpired => {
                let (connector, connection, authority, observed) = match &binding {
                    Some(grant) => (
                        Some(grant.source().connector().clone()),
                        Some(grant.source().connection_id()),
                        Some(grant.source().source_authority()),
                        Some(Self::observed_grant(grant)?),
                    ),
                    None => {
                        let (connector, connection) = known(identity).unzip();
                        (connector, connection, None, None)
                    }
                };
                Self::personal_requirement(
                    identity,
                    connector,
                    connection,
                    consumer,
                    SourceAccessRequirementKind::Reconnect,
                    authority,
                    observed,
                )?
            }
            AgentFailure::CapabilityDenied => {
                // The driver is the OS boundary: a denial past admission
                // means the system refused, so the review names system
                // permission rather than the grant. A vanished grant is drift.
                let (connector, connection, authority, observed) = match &binding {
                    Some(grant) => (
                        Some(grant.source().connector().clone()),
                        Some(grant.source().connection_id()),
                        Some(grant.source().source_authority()),
                        Some(Self::observed_grant(grant)?),
                    ),
                    None => {
                        let (connector, connection) = known(identity).unzip();
                        (connector, connection, None, None)
                    }
                };
                let reason = if binding.is_some() {
                    SourceAccessRequirementKind::RequestSystemPermission
                } else {
                    SourceAccessRequirementKind::ReviewChangedSource
                };
                Self::personal_requirement(
                    identity,
                    connector,
                    connection,
                    consumer,
                    reason,
                    authority,
                    observed,
                )?
            }
            // Mid-read drift: the grant the read started under is no longer
            // the one current authority names.
            _ => {
                let (connector, connection, authority, observed) = match &binding {
                    Some(grant) => (
                        Some(grant.source().connector().clone()),
                        Some(grant.source().connection_id()),
                        Some(grant.source().source_authority()),
                        Some(Self::observed_grant(grant)?),
                    ),
                    None => {
                        let (connector, connection) = known(identity).unzip();
                        (connector, connection, None, None)
                    }
                };
                Self::personal_requirement(
                    identity,
                    connector,
                    connection,
                    consumer,
                    SourceAccessRequirementKind::ReviewChangedSource,
                    authority,
                    observed,
                )?
            }
        };
        PersonalBlock::blocked(requirement)
    }

    /// Classify a failed people read: an admitting grant with no selection
    /// needs source selection, otherwise the personal ladder applies.
    async fn classify_people_blocker(
        &self,
        identity: &PersonalSourceIdentity,
        consumer_name: &str,
        consumer: GrantConsumer,
        error: AgentFailure,
    ) -> Result<PersonalBlock, AgentFailure> {
        if error == AgentFailure::AccessReviewRequired {
            let grants = self.records.grants().await?;
            if let Ok(grant) =
                floe_access::people_read_grant(&grants, self.person_id, &self.device_id, consumer_name)
            {
                let handles = self.records.selected_handles(grant.id()).await?;
                if handles.is_empty() {
                    let requirement = Self::personal_requirement(
                        identity,
                        Some(grant.source().connector().clone()),
                        Some(grant.source().connection_id()),
                        consumer,
                        SourceAccessRequirementKind::SelectResource,
                        Some(grant.source().source_authority()),
                        None,
                    )?;
                    // Selection happens in the picker, never inline.
                    debug_assert!(!requirement.inline_resolution());
                    return PersonalBlock::blocked(requirement);
                }
                let requirement = Self::personal_requirement(
                    identity,
                    Some(grant.source().connector().clone()),
                    Some(grant.source().connection_id()),
                    consumer,
                    SourceAccessRequirementKind::ReviewChangedSource,
                    Some(grant.source().source_authority()),
                    Some(Self::observed_grant(&grant)?),
                )?;
                return PersonalBlock::blocked(requirement);
            }
        }
        self.classify_personal_blocker(identity, consumer, error)
            .await
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
                let identity = self.personal_identity(call.tool_id.as_str())?;
                match crate::read_manager_people(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    ASSISTANT_CONSUMER,
                    deadline,
                    &cancellation,
                )
                .await
                {
                    Ok((view, dependency)) => Ok(SourceReadOutcome::Ready(Self::result(
                        call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )?)),
                    Err(error) => Ok(self
                        .classify_people_blocker(&identity, ASSISTANT_CONSUMER, consumer, error)
                        .await?
                        .into_outcome()),
                }
            }
            SCHEDULE_FEASIBILITY_READ => {
                Self::empty_input(call)?;
                let consumer = GrantConsumer::builtin(ASSISTANT_CONSUMER)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let identity = self.personal_identity(call.tool_id.as_str())?;
                match crate::read_feasibility(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    ASSISTANT_CONSUMER,
                    call.call_id,
                    deadline,
                    &cancellation,
                )
                .await
                {
                    Ok((view, dependency)) => Ok(SourceReadOutcome::Ready(Self::result(
                        call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )?)),
                    Err(error) => Ok(self
                        .classify_personal_blocker(&identity, consumer, error)
                        .await?
                        .into_outcome()),
                }
            }
            ATTENTION_COARSE_READ => {
                Self::empty_input(call)?;
                let consumer = floe_access::attention_consumer(ASSISTANT_CONSUMER)?;
                let identity = self.personal_identity(call.tool_id.as_str())?;
                match crate::admit_attention(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    consumer.clone(),
                    call.call_id,
                    deadline,
                    &cancellation,
                )
                .await
                {
                    Ok((view, dependency)) => Ok(SourceReadOutcome::Ready(Self::result(
                        call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )?)),
                    Err(error) => Ok(self
                        .classify_personal_blocker(&identity, consumer, error)
                        .await?
                        .into_outcome()),
                }
            }
            WELLBEING_DERIVED_READ => {
                Self::empty_input(call)?;
                let consumer = GrantConsumer::builtin(ASSISTANT_CONSUMER)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let identity = self.personal_identity(call.tool_id.as_str())?;
                match crate::read_wellbeing(
                    &self.records,
                    &self.driver,
                    self.person_id,
                    &self.device_id,
                    ASSISTANT_CONSUMER,
                    call.call_id,
                    deadline,
                    &cancellation,
                )
                .await
                {
                    Ok((view, dependency)) => Ok(SourceReadOutcome::Ready(Self::result(
                        call,
                        &serde_json::to_value(&view).map_err(|_| AgentFailure::InvalidInput)?,
                        dependency,
                    )?)),
                    Err(error) => Ok(self
                        .classify_personal_blocker(&identity, consumer, error)
                        .await?
                        .into_outcome()),
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

/// The owner-known identity of one personal source a direct tool reads:
///
/// the source it reports under, the resource it needs, the grant bindings
/// that count as this source, and — when the platform identity is fixed —
/// the connector/connection a missing grant still permits naming.
struct PersonalSourceIdentity {
    source_id: &'static str,
    resource: &'static str,
    expected: Vec<GrantSourceBinding>,
    known_identity: Option<(&'static str, &'static str)>,
}

/// A classified personal blocker: transiently unavailable, or blocked on a
/// concrete reviewable requirement.
enum PersonalBlock {
    Unavailable(SourceUnavailable),
    Blocked(SourceAccessBlockers),
}

impl PersonalBlock {
    fn blocked(requirement: SourceAccessRequirement) -> Result<Self, AgentFailure> {
        let blockers =
            SourceAccessBlockers::try_new(vec![requirement]).map_err(|_| AgentFailure::InvalidInput)?;
        Ok(Self::Blocked(blockers))
    }

    fn into_outcome(self) -> SourceReadOutcome<ToolResult> {
        match self {
            Self::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
            Self::Blocked(blockers) => SourceReadOutcome::NeedsUserAction(blockers),
        }
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
            // Interim: the product boundary moves to invoke_outcome with
            // trusted publication next; this mapping keeps the port total
            // without fabricating a result for a blocked read.
            match self.invoke_outcome(&call, scope).await? {
                SourceReadOutcome::Ready(result) => Ok(result),
                SourceReadOutcome::Unavailable(_) => Err(AgentFailure::CapabilityUnavailable),
                SourceReadOutcome::NeedsUserAction(_) => Err(AgentFailure::ConsentRequired),
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
                    call(
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
                    .invoke(call(tool, r#"{"extra": true}"#), &scope)
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
                call(
                    MAIL_COMMUNICATION_READ,
                    r#"{"query": "invoice", "limit": 5}"#,
                ),
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
        let paused = DataAccessGrant::new(
            GrantId::new(),
            Uuid::new_v4(),
            source,
            scope_fixture,
        )
        .unwrap();
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
            fn grants<'a>(
                &'a self,
            ) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
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

    #[tokio::test]
    async fn interim_tool_port_signals_blocked_reads_without_fabrication() {
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
        assert_eq!(
            service
                .invoke(call(ATTENTION_COARSE_READ, "{}"), &scope)
                .await
                .err(),
            Some(AgentFailure::ConsentRequired)
        );
    }
}
