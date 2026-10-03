//! Ready-generation Context adapters for the Experts-owned Engine.
use std::{collections::HashSet, sync::{Arc, Mutex}};

use chrono::Utc;
use floe_access::{DependencyAuthorization, GrantRepository, RemoteCallWindow};
use floe_agent_contract::{AgentFailure, BoxFuture, DependencyCoverage, ExecutionScope,
    OwnerActor, TaskExecutionKey};
use floe_connections::{ConnectionsRepository, SourceConnection};
use floe_context_contract::{ContextDependency, GrantConsumer, GrantPurpose, PersonId,
    SourceReadOutcome, SourceSelectionReference};
use floe_experts::{ExpertManifest, ExpertSourcePort, ExpertSourceRead, ExpertSourceRequest};
use serde_json::Value;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{CalendarConnectionReader, CalendarObserveRequest, CalendarSource,
    DeclaredSourceRequirement, DependencyResolver, EvidenceReader, ExpertSourceTransport,
    LocalExpertSource, LocalExpertSourceDriver, NativeCalendarGrantReader,
    PersonalConnectionReader, PersonalSourceDriver, SelectedSourceReader, SourceLeaseRegistry,
    SourceLeaseReservation, SourceRead, SourceReadRequest, SourceView};

/// These handles belong to one opened profile generation. Host-lifetime Day
/// acquisition remains independently usable without these encrypted readers.
pub struct ExpertContextDependencies {
    pub actor: OwnerActor,
    pub manifests: Vec<ExpertManifest>,
    pub connections: Arc<dyn ConnectionsRepository>,
    pub grants: Arc<dyn GrantRepository>,
    pub personal: Arc<dyn PersonalSourceDriver + Send>,
    pub transport: Arc<dyn ExpertSourceTransport>,
    pub day: Arc<dyn floe_day::DayRepository>,
    pub evidence: Arc<dyn EvidenceReader>,
    pub resolver: Arc<dyn DependencyResolver>,
    pub leases: Arc<SourceLeaseRegistry>,
}

impl ExpertContextDependencies {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        if self.manifests.len() > 64 { return Err(AgentFailure::BudgetExceeded); }
        let mut packages = HashSet::new();
        for manifest in &self.manifests {
            manifest.validate()?;
            if !packages.insert((&manifest.package.id, &manifest.package.version)) {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(())
    }

    pub(crate) fn authorize_actor(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        if actor != &self.actor { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }

    fn authorize_task(&self, actor: &OwnerActor, execution: TaskExecutionKey,
        scope: &ExecutionScope) -> Result<(), AgentFailure>
    {
        self.authorize_actor(actor)?;
        execution.validate()?;
        if scope.task_id() != Some(execution.task_id) { return Err(AgentFailure::PolicyDenied); }
        check_scope(scope)
    }
}

pub struct ContextExpertSources {
    dependencies: Arc<ExpertContextDependencies>,
    memory: Arc<dyn floe_knowledge::KnowledgeRead>,
}

pub struct ContextExpertProjection {
    dependencies: Arc<ExpertContextDependencies>,
    memory: Arc<dyn floe_knowledge::KnowledgeRead>,
}

impl ContextExpertProjection {
    pub fn new(dependencies: Arc<ExpertContextDependencies>, memory: Arc<dyn floe_knowledge::KnowledgeRead>)
        -> Result<Self, AgentFailure>
    {
        dependencies.validate()?;
        Ok(Self { dependencies, memory })
    }
}

impl floe_experts::ExpertProjectionPort for ContextExpertProjection {
    fn project<'a>(&'a self, mut input: floe_experts::ExpertProjectionRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<floe_agent_contract::ModelProjectionOutcome, AgentFailure>>
    {
        Box::pin(async move {
            use floe_agent_contract::{DataClass, ModelConversationEntry};
            let dependencies = self.dependencies.as_ref();
            dependencies.authorize_task(&input.actor, input.execution, scope)?;
            input.request.validate()?;
            input.prompt.validate()?;
            let request = &input.request;
            if request.principal != input.actor.person_id.to_string()
                || request.plan.device_id != input.actor.device_id
                || request.plan.consumer != floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER
                || request.plan.purpose != "everyday_assistance"
                || request.role.instructions != input.prompt.render()
                || !request.catalog.cards.is_empty() || !request.conversation.history.is_empty()
                || input.observations.len() > 32
                || matches!(input.package_data_class, DataClass::Credential | DataClass::DeviceOnlyRaw)
            { return Err(AgentFailure::PolicyDenied); }
            let mut manifests = dependencies.manifests.iter().filter(|manifest|
                manifest.package.id == request.role.role_id
                    && manifest.definition.definition_revision == request.catalog.revision);
            let manifest = manifests.next().ok_or(AgentFailure::CapabilityDenied)?;
            if manifests.next().is_some() || manifest.data_class != input.package_data_class {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut tool_ids = HashSet::new();
            for tool in &request.catalog.tools {
                if tool.definition_revision != manifest.definition.definition_revision
                    || !manifest.source_requirements.iter().any(|requirement| requirement.key == tool.id)
                    || !tool_ids.insert(&tool.id)
                { return Err(AgentFailure::CapabilityDenied); }
            }
            authorize_coverage(dependencies, &input.inherited_coverage, scope).await?;
            let mut coverage = input.inherited_coverage.clone();
            let mut seen = HashSet::new();
            let mut seen_calls = HashSet::new();
            for entry in &request.conversation.current_turn {
                match entry {
                    ModelConversationEntry::ToolExchange { call, result } => {
                        if !seen_calls.insert(call.call_id) { return Err(AgentFailure::PolicyDenied); }
                        let Some(observation) = input.observations.iter().find(|observation| observation.call == *call) else {
                            validate_tool_correction(&request.catalog, call, result)?;
                            continue;
                        };
                        if observation.requirement_key != call.tool_id || !seen.insert(call.call_id)
                            || !tool_ids.contains(&call.tool_id) || !result.artifacts.is_empty()
                            || result.coverage != observation.coverage()
                        { return Err(AgentFailure::PolicyDenied); }
                        match &observation.outcome {
                            floe_experts::ExpertSourceObservation::Ready { payload, .. } => {
                                let projected: Value = serde_json::from_str(&result.text)
                                    .map_err(|_| AgentFailure::PolicyDenied)?;
                                if &projected != payload || result.issue.is_some() {
                                    return Err(AgentFailure::PolicyDenied);
                                }
                            }
                            floe_experts::ExpertSourceObservation::Unavailable { .. } => {
                                if result.text != "Source is unavailable for this read."
                                    || result.issue.as_ref().is_none_or(|issue|
                                        issue.failure != AgentFailure::CapabilityUnavailable || issue.retryable)
                                { return Err(AgentFailure::PolicyDenied); }
                            }
                        }
                        let observed = observation.coverage();
                        authorize_coverage(dependencies, &observed, scope).await?;
                        coverage = coverage.merge(&observed).map_err(|_| AgentFailure::PolicyDenied)?;
                    }
                    ModelConversationEntry::DelegationExchange { .. } => return Err(AgentFailure::CapabilityDenied),
                    _ => {}
                }
            }
            if seen.len() != input.observations.len() { return Err(AgentFailure::PolicyDenied); }
            let (memory, memory_coverage) = read_memory(dependencies, self.memory.as_ref(), scope).await?;
            input.context.memories = memory.memories;
            floe_context_contract::record_source_issue(&mut input.context.optional_context_issues,
                floe_agent_contract::ContextSource::Memory, memory.issue);
            coverage = coverage.merge(&memory_coverage).map_err(|_| AgentFailure::PolicyDenied)?;
            let mut classes = vec![DataClass::Personal, input.package_data_class];
            classes.extend(input.context.evidence.iter().map(|evidence| evidence.data_class));
            if let DependencyCoverage::Dependent { dependencies: values } = &coverage {
                if values.iter().any(|dependency|
                    dependency.source().connector().as_str() == floe_access::WELLBEING_CONNECTOR)
                { classes.push(DataClass::HighlySensitive); }
            }
            classes.sort();
            classes.dedup();
            if classes.iter().any(|class| matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw)) {
                return Err(AgentFailure::PolicyDenied);
            }
            let now = Utc::now();
            let policy = floe_agent_contract::InferencePolicyDecision {
                purpose: request.plan.purpose.clone(), data_classes: classes.clone(),
                performance_class: "interactive".into(), projection_version: input.context.projection_version,
            };
            scope.run(crate::prepare_expert_context(&mut input.context, dependencies.day.as_ref(),
                crate::ExpertContextRequest { person_id: input.actor.person_id, policy: &policy,
                    protection: floe_agent_contract::SessionProtection::Encrypted, now,
                    deadline: scope.deadline(), cancellation: scope.cancellation().clone() })).await?;
            let authorized = dependency_values(coverage)?;
            check_scope(scope)?;
            crate::assemble_context_projection(crate::ContextProjectionInput {
                role: crate::ContextProjectionRole::Expert, plan: &request.plan,
                projection_operation_id: request.projection_operation_id, purpose: &request.plan.purpose,
                response_contract: &request.role.output_contract, correction: request.correction.clone(),
                prompt: input.prompt, conversation: request.conversation.clone(), agent_context: &input.context,
                catalog: &request.catalog, expert_environment: None, authorized_history_dependencies: &authorized,
                input_data_classes: classes, max_output_bytes: request.max_output_bytes,
            })
        })
    }
}

/// The Engine may reject a call before the source port runs. Only its exact
/// fixed, source-free correction is admitted without a source observation.
fn validate_tool_correction(catalog: &floe_agent_contract::AllowedCatalog,
    call: &floe_agent_contract::ToolCall, result: &floe_agent_contract::ToolResult)
    -> Result<(), AgentFailure>
{
    floe_agent_contract::validate_tool_input(&call.input)?;
    let expected = match catalog.tools.iter().find(|tool| tool.id == call.tool_id) {
        None => "tool is not registered",
        Some(tool) if tool.definition_revision != call.definition_revision => "tool descriptor is stale",
        Some(tool) => {
            let value: Value = serde_json::from_str(&call.input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            let schema: Value = serde_json::from_str(&tool.input_schema).map_err(|_| AgentFailure::InvalidInput)?;
            let validator = jsonschema::validator_for(&schema).map_err(|_| AgentFailure::InvalidInput)?;
            if validator.is_valid(&value) { return Err(AgentFailure::PolicyDenied); }
            "tool arguments do not satisfy the registered input schema"
        }
    };
    if result.call_id != call.call_id || result.text != expected || !result.artifacts.is_empty()
        || result.coverage != DependencyCoverage::Independent
        || result.issue.as_ref().is_none_or(|issue|
            issue.failure != AgentFailure::InvalidModelOutput || !issue.retryable)
    { return Err(AgentFailure::PolicyDenied); }
    Ok(())
}

impl ContextExpertSources {
    pub fn new(dependencies: Arc<ExpertContextDependencies>, memory: Arc<dyn floe_knowledge::KnowledgeRead>)
        -> Result<Self, AgentFailure>
    {
        dependencies.validate()?;
        Ok(Self { dependencies, memory })
    }
}

impl ExpertSourcePort for ContextExpertSources {
    fn read<'a>(&'a self, request: ExpertSourceRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<SourceReadOutcome<ExpertSourceRead>, AgentFailure>>
    {
        Box::pin(async move {
            let dependencies = self.dependencies.as_ref();
            dependencies.authorize_task(&request.actor, request.execution, scope)?;
            let manifest = dependencies.manifests.iter().find(|manifest|
                manifest.package == request.admission.package
                    && manifest.definition.definition_revision == request.admission.definition_revision)
                .ok_or(AgentFailure::CapabilityDenied)?;
            request.admission.validate(&manifest.definition)?;
            let declared = manifest.source_requirements.iter()
                .find(|item| item.key == request.requirement.key)
                .ok_or(AgentFailure::CapabilityDenied)?;
            let requirement = &request.requirement;
            if requirement.capability != declared.capability
                || requirement.contract_version != declared.contract_version
                || requirement.minimum_sources != declared.minimum_sources
                || requirement.maximum_sources != declared.maximum_sources
                || requirement.selected.len() < usize::from(declared.minimum_sources)
                || requirement.selected.len() > usize::from(declared.maximum_sources)
                || requirement.selected.windows(2).any(|pair| pair[0] >= pair[1])
                || request.call.call_id.is_nil()
                || request.call.invocation_key.as_uuid().is_nil()
                || request.call.tool_id != requirement.key
                || request.call.definition_revision != request.admission.definition_revision
                || request.call.input.len() > 65_536
                || request.max_output_bytes == 0
                || request.max_output_bytes > floe_agent_contract::MAX_OUTPUT_BYTES
            { return Err(AgentFailure::CapabilityDenied); }
            for selected in &requirement.selected {
                selected.validate().map_err(|_| AgentFailure::InvalidInput)?;
                if selected.capability_id != requirement.capability
                    || selected.contract_version != requirement.contract_version
                { return Err(AgentFailure::CapabilityDenied); }
            }
            let query: Value = serde_json::from_str(&request.call.input)
                .map_err(|_| AgentFailure::InvalidInput)?;
            if !query.is_object() { return Err(AgentFailure::InvalidInput); }
            let remote = SelectedRemote { dependencies, scope };
            let local = LocalSources { dependencies, memory: self.memory.as_ref(), scope,
                consumer: &manifest.package.id, call_id: request.call.call_id,
                held: Mutex::new(Vec::new()) };
            let declared = [DeclaredSourceRequirement { key: &requirement.key,
                capability: &requirement.capability, contract_version: requirement.contract_version,
                selected_refs: &requirement.selected }];
            let outcome = scope.run(crate::read_declared_source(Some(&remote), &local,
                request.actor.person_id, &manifest.package.id, &declared, &requirement.key,
                query, scope.deadline(), scope.cancellation())).await?;
            check_scope(scope)?;
            match outcome {
                SourceReadOutcome::Ready(read) => {
                    let bytes = super::source_view::bounded_serialized_size(&read.payload,
                        request.max_output_bytes)?;
                    let coverage = coverage_from_dependencies(read.dependencies)?;
                    authorize_coverage(dependencies, &coverage, scope).await?;
                    let reservation = dependencies.leases.reserve(request.actor.person_id, bytes)?;
                    let held = local.held.into_inner().map_err(|_| AgentFailure::StorageUnavailable)?;
                    if read.held.as_ref().is_some_and(|view| !view.is_fresh())
                        || held.iter().any(|view| !view.is_fresh())
                    { return Err(AgentFailure::StaleContext); }
                    check_scope(scope)?;
                    let retention = SourceRetention { _remote: read.held, _local: held,
                        _reservation: reservation };
                    Ok(SourceReadOutcome::Ready(ExpertSourceRead::new(read.payload, coverage,
                        Box::new(retention))?))
                }
                SourceReadOutcome::NeedsUserAction(blockers) => {
                    blockers.validate().map_err(|_| AgentFailure::PolicyDenied)?;
                    let consumer = GrantConsumer::builtin(&manifest.package.id)
                        .map_err(|_| AgentFailure::PolicyDenied)?;
                    for blocker in blockers.blockers() {
                        if blocker.consumer() != &consumer || blocker.purpose() != GrantPurpose::Assistant
                            || blocker.source_id() != floe_context_contract::source_access_id_for_capability(
                                &requirement.capability).ok_or(AgentFailure::CapabilityDenied)?
                            || !requirement.selected.iter().any(|selection|
                                blocker.connection_id() == Some(&selection.connection_id)
                                    && blocker.connector_id() == Some(&selection.connector_id)
                                    && blocker.resources().contains(&selection.resource))
                        { return Err(AgentFailure::PolicyDenied); }
                    }
                    Ok(SourceReadOutcome::NeedsUserAction(blockers))
                }
                SourceReadOutcome::Unavailable(reason) => Ok(SourceReadOutcome::Unavailable(reason)),
            }
        })
    }
}

struct SourceRetention {
    _remote: Option<SourceView<Value>>,
    _local: Vec<SourceView<Value>>,
    _reservation: SourceLeaseReservation,
}

struct SelectedRemote<'a> {
    dependencies: &'a ExpertContextDependencies,
    scope: &'a ExecutionScope,
}
impl SelectedSourceReader for SelectedRemote<'_> {
    fn read_selected<'a>(&'a self, request: &'a SourceReadRequest,
        selected: &'a [SourceSelectionReference])
        -> BoxFuture<'a, Result<SourceReadOutcome<SourceRead>, AgentFailure>>
    {
        Box::pin(async move {
            if request.person_id() != self.dependencies.actor.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let remote = self.dependencies.transport.remote(&self.dependencies.actor, self.scope)
                .await?.ok_or(AgentFailure::CapabilityUnavailable)?;
            let person = request.person_id().to_string();
            let pairing = floe_access::RemotePairingIdentity { person_id: &person,
                client_id: remote.transport.client_id(), device_id: &self.dependencies.actor.device_id };
            let outcome = crate::read_selected_remote_view(self.dependencies.grants.as_ref(),
                remote.verifier.as_ref(), self.dependencies.connections.as_ref(),
                self.dependencies.connections.as_ref(), remote.transport.as_ref(), request.person_id(),
                pairing, request.source().as_str(), request.consumer().identifier(), selected,
                request.query().clone(), &RemoteCallWindow { deadline: request.deadline(),
                    cancellation: request.cancellation().clone() }, request.process_incarnation_id(),
                request.query_fingerprint()).await?;
            Ok(match outcome {
                SourceReadOutcome::Ready((payload, bindings)) => SourceReadOutcome::Ready(
                    SourceRead::with_bindings(request.source().clone(), payload, bindings)),
                SourceReadOutcome::NeedsUserAction(blockers) => SourceReadOutcome::NeedsUserAction(blockers),
                SourceReadOutcome::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
            })
        })
    }
}

struct LocalSources<'a> {
    dependencies: &'a ExpertContextDependencies,
    memory: &'a dyn floe_knowledge::KnowledgeRead,
    scope: &'a ExecutionScope,
    consumer: &'a str,
    call_id: Uuid,
    held: Mutex<Vec<SourceView<Value>>>,
}

impl LocalExpertSourceDriver for LocalSources<'_> {
    fn read<'a>(&'a self, source: LocalExpertSource, source_access_id: &'static str,
        selected: &'a [SourceSelectionReference], query: Value, deadline: Instant,
        cancellation: &'a floe_execution::Cancellation)
        -> BoxFuture<'a, Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>>
    {
        Box::pin(async move {
            let actor = &self.dependencies.actor;
            let singleton = || match selected { [selected] => Ok(selected),
                _ => Err(AgentFailure::CapabilityDenied) };
            let connections = PersonalConnections(self.dependencies.connections.as_ref());
            match source {
                LocalExpertSource::Calendar => self.calendar(source_access_id, selected, query).await,
                LocalExpertSource::People => map_personal(crate::read_selected_people_outcome(
                    &connections, self.dependencies.grants.as_ref(), self.dependencies.personal.as_ref(),
                    actor.person_id, &actor.device_id, singleton()?, self.consumer, deadline, cancellation).await?),
                LocalExpertSource::Wellbeing => map_personal(crate::read_selected_wellbeing_outcome(
                    &connections, self.dependencies.grants.as_ref(), self.dependencies.personal.as_ref(),
                    actor.person_id, &actor.device_id, singleton()?, self.consumer, self.call_id,
                    deadline, cancellation).await?),
                LocalExpertSource::Attention => map_personal(crate::admit_selected_attention_outcome(
                    &connections, self.dependencies.grants.as_ref(), self.dependencies.personal.as_ref(),
                    actor.person_id, &actor.device_id, singleton()?, floe_access::attention_consumer(self.consumer)?,
                    self.call_id, deadline, cancellation).await?),
                LocalExpertSource::ConfirmedMemory => {
                    crate::validate_local_source_selection(singleton()?, &actor.device_id)?;
                    let (snapshot, coverage) = read_memory(self.dependencies, self.memory, self.scope).await?;
                    Ok(SourceReadOutcome::Ready((serde_json::json!({"memories": snapshot.memories,
                        "issue": snapshot.issue}), dependency_values(coverage)?)))
                }
                LocalExpertSource::Tasks => {
                    crate::validate_local_source_selection(singleton()?, &actor.device_id)?;
                    let view = crate::task_context_view(self.dependencies.day.as_ref(), actor.person_id,
                        Uuid::new_v5(&actor.person_id.0, b"floe.tasks"), Utc::now(), 16, 8 * 1024).await?;
                    Ok(SourceReadOutcome::Ready((serde_json::to_value(view)
                        .map_err(|_| AgentFailure::InvalidInput)?, Vec::new())))
                }
                // No implementation currently admits this capability. An
                // unsupported source is never represented as successful data.
                LocalExpertSource::ConfirmedInteractions => Err(AgentFailure::CapabilityUnavailable),
            }
        })
    }
}

impl LocalSources<'_> {
    async fn calendar(&self, source_access_id: &str, selected: &[SourceSelectionReference], query: Value)
        -> Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>
    {
        if selected.is_empty() { return Err(AgentFailure::CapabilityDenied); }
        let calendar: floe_context_contract::CalendarViewQuery = serde_json::from_value(query.clone())
            .map_err(|_| AgentFailure::InvalidInput)?;
        calendar.validate()?;
        let mut views = Vec::new();
        let mut dependencies = Vec::new();
        let mut blockers = Vec::new();
        let mut unavailable = None;
        for selection in selected {
            check_scope(self.scope)?;
            let actor = &self.dependencies.actor;
            let connection = self.dependencies.connections.load(actor.person_id, &selection.connection_id)
                .await.map_err(|_| AgentFailure::StorageUnavailable)?.ok_or(AgentFailure::StaleContext)?;
            let logical = floe_context_contract::connection_view_resource("calendar.timeline", connection.connection_id())
                .map_err(|_| AgentFailure::InvalidInput)?;
            if connection.person_id() != actor.person_id || !connection.is_serving()
                || crate::current_calendar_connector(&connection).is_none()
                || selection.connector_id != *connection.connector_id()
                || selection.execution_owner_id != *connection.execution_owner_id()
                || selection.resource != logical
                || selection.capability_id != "calendar.timeline" || selection.contract_version != 1
            { return Err(AgentFailure::StaleContext); }
            let outcome = if connection.connector_id().as_str() == "calendar.event_kit" {
                if connection.execution_owner_id().as_str() != actor.device_id {
                    return Err(AgentFailure::PolicyDenied);
                }
                let connections = CalendarConnections { repository: self.dependencies.connections.as_ref(),
                    person_id: actor.person_id, connection_id: &selection.connection_id };
                let source = NativeCalendar { actor, connection: &connection,
                    transport: self.dependencies.transport.as_ref() };
                let grants = NativeGrants(self.dependencies.grants.as_ref());
                let window = RemoteCallWindow { deadline: self.scope.deadline(),
                    cancellation: self.scope.cancellation().clone() };
                let result = crate::read_native_calendar_view(&connections, &source, &grants,
                    &self.dependencies.leases, crate::NativeCalendarViewRead { person_id: actor.person_id,
                        device_id: &actor.device_id, consumer: self.consumer, query: &calendar, window: &window }).await;
                match result {
                    Ok((view, dependency)) => SourceReadOutcome::Ready((view, dependency)),
                    Err(AgentFailure::CapabilityUnavailable) => {
                        SourceReadOutcome::Unavailable(floe_context_contract::SourceUnavailable::TemporarilyUnavailable)
                    }
                    Err(failure @ (AgentFailure::AccessReviewRequired | AgentFailure::CredentialExpired)) => {
                        self.calendar_review(source_access_id, &connection, selection, failure).await?
                    }
                    Err(failure) => return Err(failure),
                }
            } else {
                let prepared = crate::ContextService::new(None).prepare(actor.person_id)?;
                let request = prepared.source_request("calendar.timeline", GrantConsumer::builtin(self.consumer)
                    .map_err(|_| AgentFailure::InvalidInput)?, GrantPurpose::Assistant, query.clone(),
                    self.scope.deadline(), self.scope.cancellation().clone())?;
                let remote = SelectedRemote { dependencies: self.dependencies, scope: self.scope };
                match prepared.read_selected_source(&request, &remote, std::slice::from_ref(selection)).await? {
                    SourceReadOutcome::Ready(read) => {
                        let view = serde_json::from_value(read.payload().clone()).map_err(|_| AgentFailure::InvalidInput)?;
                        if read.bindings().len() != 1 { return Err(AgentFailure::Conflict); }
                        let dependency = read.bindings()[0].dependency.clone();
                        self.held.lock().map_err(|_| AgentFailure::StorageUnavailable)?.push(read);
                        SourceReadOutcome::Ready((view, dependency))
                    }
                    SourceReadOutcome::NeedsUserAction(blockers) => SourceReadOutcome::NeedsUserAction(blockers),
                    SourceReadOutcome::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
                }
            };
            match outcome {
                SourceReadOutcome::Ready((view, dependency)) => {
                    crate::validate_calendar_context_view_for_query(&view, &calendar, Utc::now().timestamp_millis())?;
                    views.push(view);
                    dependencies.push(dependency);
                }
                SourceReadOutcome::NeedsUserAction(required) => {
                    for blocker in required.blockers() {
                        if !blockers.contains(blocker) { blockers.push(blocker.clone()); }
                    }
                }
                SourceReadOutcome::Unavailable(reason) => unavailable = Some(reason),
            }
        }
        if !blockers.is_empty() {
            return Ok(SourceReadOutcome::NeedsUserAction(floe_context_contract::SourceAccessBlockers::try_new(blockers)
                .map_err(|_| AgentFailure::PolicyDenied)?));
        }
        if let Some(reason) = unavailable { return Ok(SourceReadOutcome::Unavailable(reason)); }
        Ok(SourceReadOutcome::Ready((serde_json::to_value(views).map_err(|_| AgentFailure::InvalidInput)?, dependencies)))
    }

    async fn calendar_review<Value>(&self, source_access_id: &str, connection: &SourceConnection,
        selected: &SourceSelectionReference, failure: AgentFailure) -> Result<SourceReadOutcome<Value>, AgentFailure>
    {
        let source = floe_context_contract::GrantSourceBinding::try_new(self.dependencies.actor.person_id,
            connection.connection_id().clone(), connection.connector_id().clone(), connection.execution_owner_id().clone())
            .map_err(|_| AgentFailure::InvalidInput)?;
        let grants = self.dependencies.grants.snapshot(source).await?.grants;
        let review = crate::classify_calendar_review(&grants, self.dependencies.actor.person_id,
            connection.connector_id().as_str(), connection.connection_id().as_str())?;
        let reason = if failure == AgentFailure::CredentialExpired {
            floe_context_contract::SourceAccessRequirementKind::Reconnect
        } else { review.reason };
        let requirement = floe_context_contract::SourceAccessRequirement::try_new(source_access_id,
            Some(connection.connector_id().clone()), Some(connection.connection_id().clone()),
            floe_context_contract::GrantOperation::Read, GrantConsumer::builtin(self.consumer)
                .map_err(|_| AgentFailure::InvalidInput)?, GrantPurpose::Assistant,
            vec![selected.resource.clone()], None, reason, Some(connection.source_authority()), review.observed,
            reason != floe_context_contract::SourceAccessRequirementKind::Reconnect)
            .map_err(|_| AgentFailure::StaleContext)?;
        Ok(SourceReadOutcome::NeedsUserAction(floe_context_contract::SourceAccessBlockers::try_new(vec![requirement])
            .map_err(|_| AgentFailure::StaleContext)?))
    }
}

struct PersonalConnections<'a>(&'a dyn ConnectionsRepository);
impl PersonalConnectionReader for PersonalConnections<'_> {
    fn source_is_fenced<'a>(&'a self, person: PersonId, connection: &'a floe_context_contract::ConnectionId)
        -> BoxFuture<'a, Result<bool, AgentFailure>>
    { Box::pin(async move { self.0.source_is_fenced(person, connection).await.map_err(|_| AgentFailure::StorageUnavailable) }) }
    fn load<'a>(&'a self, person: PersonId, connection: &'a floe_context_contract::ConnectionId)
        -> BoxFuture<'a, Result<Option<SourceConnection>, AgentFailure>>
    { Box::pin(async move { self.0.load(person, connection).await.map_err(|_| AgentFailure::StorageUnavailable) }) }
}

struct CalendarConnections<'a> { repository: &'a dyn ConnectionsRepository,
    person_id: PersonId, connection_id: &'a floe_context_contract::ConnectionId }
impl CalendarConnectionReader for CalendarConnections<'_> {
    async fn source_is_fenced(&self, person: PersonId, connection: &floe_context_contract::ConnectionId)
        -> Result<bool, AgentFailure>
    { self.repository.source_is_fenced(person, connection).await.map_err(|_| AgentFailure::StorageUnavailable) }
    async fn calendar_connection(&self) -> Result<Option<SourceConnection>, AgentFailure>
    { self.repository.load(self.person_id, self.connection_id).await.map_err(|_| AgentFailure::StorageUnavailable) }
}

struct NativeCalendar<'a> { actor: &'a OwnerActor, connection: &'a SourceConnection,
    transport: &'a dyn ExpertSourceTransport }
impl CalendarSource for NativeCalendar<'_> {
    async fn check(&self, request: floe_access::CalendarReadAccessRequest)
        -> Result<floe_access::CalendarReadAccessStamp, AgentFailure>
    { self.transport.check_calendar(self.actor, self.connection, request).await }
    async fn observe(&self, request: CalendarObserveRequest) -> Result<Option<crate::CalendarObservation>, AgentFailure>
    { self.transport.observe_calendar(self.actor, self.connection, request).await.map(Some) }
}
struct NativeGrants<'a>(&'a dyn GrantRepository);
impl NativeCalendarGrantReader for NativeGrants<'_> {
    async fn admit(&self, connection: &SourceConnection, person: PersonId, consumer: &str)
        -> Result<floe_access::CalendarReadAccessAdmission, AgentFailure>
    {
        let consumer = GrantConsumer::builtin(consumer).map_err(|_| AgentFailure::CapabilityDenied)?;
        let grant = floe_access::current_native_calendar_grant(self.0, person, connection.connection_id().as_str(),
            floe_context_contract::CalendarProvider::EventKit, connection.execution_owner_id().as_str(), &consumer).await?;
        Ok(floe_access::CalendarReadAccessAdmission::device_local(person, grant.id(), grant.authority(),
            grant.source().clone(), connection.source_authority(), grant.scope().clone(), consumer))
    }
}

fn map_personal<T: serde::Serialize>(outcome: SourceReadOutcome<(T, ContextDependency)>)
    -> Result<SourceReadOutcome<(Value, Vec<ContextDependency>)>, AgentFailure>
{
    Ok(match outcome {
        SourceReadOutcome::Ready((view, dependency)) => SourceReadOutcome::Ready((serde_json::to_value(view)
            .map_err(|_| AgentFailure::InvalidInput)?, vec![dependency])),
        SourceReadOutcome::NeedsUserAction(blockers) => SourceReadOutcome::NeedsUserAction(blockers),
        SourceReadOutcome::Unavailable(reason) => SourceReadOutcome::Unavailable(reason),
    })
}

async fn read_memory(dependencies: &ExpertContextDependencies,
    memory: &dyn floe_knowledge::KnowledgeRead, scope: &ExecutionScope)
    -> Result<(floe_context_contract::MemoryContextSnapshot, DependencyCoverage), AgentFailure>
{
    let mut snapshot = scope.run(memory.read_context(&dependencies.actor, scope)).await?;
    let mut retained = Vec::new();
    let mut total = DependencyCoverage::Independent;
    for memory in snapshot.memories {
        let mut coverage = DependencyCoverage::Independent;
        if memory.source_refs.is_empty() { return Err(AgentFailure::PolicyDenied); }
        for source in &memory.source_refs {
            let source_coverage = scope.run(dependencies.evidence.read_turn_coverage(source.session_id, source.turn_id)).await?;
            coverage = coverage.merge(&source_coverage).map_err(|_| AgentFailure::PolicyDenied)?;
        }
        match authorize_coverage(dependencies, &coverage, scope).await {
            Ok(()) => { total = total.merge(&coverage).map_err(|_| AgentFailure::PolicyDenied)?; retained.push(memory); }
            Err(AgentFailure::PolicyDenied | AgentFailure::AccessReviewRequired | AgentFailure::StaleContext) => {}
            Err(failure) => return Err(failure),
        }
    }
    snapshot.memories = retained;
    Ok((snapshot, total))
}

async fn authorize_coverage(dependencies: &ExpertContextDependencies, coverage: &DependencyCoverage,
    scope: &ExecutionScope) -> Result<(), AgentFailure>
{
    coverage.validate().map_err(|_| AgentFailure::PolicyDenied)?;
    if let DependencyCoverage::Dependent { dependencies: values } = coverage {
        if values.iter().any(|value| value.person_id() != dependencies.actor.person_id) {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    scope.run(crate::revalidate_turn_coverage(coverage.clone(), dependencies.resolver.as_ref(),
        &DependencyAuthorization { deadline: scope.deadline(), cancellation: scope.cancellation().clone() })).await
}

fn coverage_from_dependencies(values: Vec<ContextDependency>) -> Result<DependencyCoverage, AgentFailure> {
    values.into_iter().try_fold(DependencyCoverage::Independent, |coverage, dependency|
        coverage.merge(&DependencyCoverage::Dependent { dependencies: vec![dependency] })
            .map_err(|_| AgentFailure::PolicyDenied))
}
fn dependency_values(coverage: DependencyCoverage) -> Result<Vec<ContextDependency>, AgentFailure> {
    match coverage { DependencyCoverage::Independent => Ok(Vec::new()),
        DependencyCoverage::Dependent { dependencies } => Ok(dependencies),
        DependencyCoverage::Unknown => Err(AgentFailure::PolicyDenied) }
}
pub(crate) fn check_scope(scope: &ExecutionScope) -> Result<(), AgentFailure> {
    if scope.cancellation().is_cancelled() { return Err(AgentFailure::Cancelled); }
    if scope.deadline() <= Instant::now() { return Err(AgentFailure::DeadlineExceeded); }
    Ok(())
}
