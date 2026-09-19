use std::future::Future;

use floe_inference::{
    DataRecipient, ExecutionLocation, InferenceRouter, ModelCapabilities, ModelConsumer,
    ModelProfile, ModelPurpose, PlannedRoute, RecipientConstraint, RoutePlanError, RouteRequest,
};
use floe_kernel::AgentFailure;
use serde::Deserialize;

use super::learner::{
    LearnerMemoryProposal, LearnerModel, LearnerModelRequest, LearnerReviewOutput,
    validate_learner_input,
};
use crate::KNOWLEDGE_VERSION;

pub const LEARNER_INFERENCE_PURPOSE: &str = "governed-memory-review";
pub const LEARNER_INFERENCE_CONSUMER: &str = "knowledge.learner";
pub const LEARNER_INFERENCE_CAPABILITY: &str = "structured-memory-review";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnerInferenceResponse {
    pub text: String,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

pub trait LearnerInferenceTransport: Sync {
    fn profile(&self) -> Result<ModelProfile, AgentFailure>;

    fn generate(
        &self,
        route: PlannedRoute,
        request: LearnerModelRequest,
    ) -> impl Future<Output = Result<LearnerInferenceResponse, AgentFailure>> + Send;
}

pub struct InferenceLearnerModel<Transport> {
    transport: Transport,
}

impl<Transport> InferenceLearnerModel<Transport> {
    pub const fn new(transport: Transport) -> Self {
        Self { transport }
    }
}

impl<Transport: LearnerInferenceTransport> LearnerModel for InferenceLearnerModel<Transport> {
    fn placement(&self) -> floe_context_contract::ModelPlacement {
        floe_context_contract::ModelPlacement::DeviceLocal
    }

    async fn review(
        &self,
        request: LearnerModelRequest,
    ) -> Result<LearnerReviewOutput, AgentFailure> {
        validate_request(&request)?;
        let profile = self.transport.profile()?;
        let route = plan_route(profile)?;
        let response = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(request.deadline) => Err(AgentFailure::DeadlineExceeded),
            response = self.transport.generate(route, request.clone()) => response,
        }?;
        parse_response(response, &request)
    }
}

fn validate_request(request: &LearnerModelRequest) -> Result<(), AgentFailure> {
    validate_learner_input(&request.input, request.input.person_id)?;
    if request.remaining_tokens == 0
        || request.remaining_cost_micros == 0
        || request.max_output_bytes == 0
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    if request.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if request.deadline <= tokio::time::Instant::now() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    if request.input.turn_ids.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn plan_route(profile: ModelProfile) -> Result<PlannedRoute, AgentFailure> {
    let profile_id = profile.id.clone();
    let purpose = ModelPurpose::new(LEARNER_INFERENCE_PURPOSE).ok_or(AgentFailure::InvalidInput)?;
    let consumer =
        ModelConsumer::new(LEARNER_INFERENCE_CONSUMER).ok_or(AgentFailure::InvalidInput)?;
    let request = RouteRequest {
        purpose,
        requested_capabilities: ModelCapabilities(vec![LEARNER_INFERENCE_CAPABILITY.into()]),
        consumer,
        recipient: RecipientConstraint::DeviceOnly,
        preferred_profile_id: Some(profile_id),
    };
    let route = InferenceRouter::new([profile])
        .map_err(map_route_error)?
        .plan(&request)
        .map_err(map_route_error)?;
    if route.execution_location != ExecutionLocation::Device
        || route.data_recipient != DataRecipient::Device
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(route)
}

fn map_route_error(error: RoutePlanError) -> AgentFailure {
    match error {
        RoutePlanError::NotConfigured => AgentFailure::ModelUnavailable,
        RoutePlanError::ConsentRequired => AgentFailure::ConsentRequired,
        RoutePlanError::Unavailable => AgentFailure::ModelUnavailable,
        RoutePlanError::Denied => AgentFailure::PolicyDenied,
    }
}

fn parse_response(
    response: LearnerInferenceResponse,
    request: &LearnerModelRequest,
) -> Result<LearnerReviewOutput, AgentFailure> {
    if response.used_tokens > request.remaining_tokens
        || response.cost_micros > request.remaining_cost_micros
        || response.text.len() > request.max_output_bytes
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    let value: serde_json::Value =
        serde_json::from_str(&response.text).map_err(|_| AgentFailure::InvalidModelOutput)?;
    if !value
        .as_object()
        .is_some_and(|object| object.contains_key("proposal"))
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let answer: StructuredLearnerAnswer =
        serde_json::from_str(&response.text).map_err(|_| AgentFailure::InvalidModelOutput)?;
    if answer.schema_version != KNOWLEDGE_VERSION {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(LearnerReviewOutput {
        schema_version: answer.schema_version,
        proposal: answer.proposal,
        used_tokens: response.used_tokens,
        cost_micros: response.cost_micros,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructuredLearnerAnswer {
    schema_version: u32,
    proposal: Option<LearnerMemoryProposal>,
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::Utc;
    use floe_inference::{DataRecipient, ExecutionLocation, ModelCapabilities};
    use floe_kernel::PersonId;
    use tokio::time::{Duration, Instant};
    use uuid::Uuid;

    use super::*;
    use crate::{KNOWLEDGE_VERSION, LearnerReviewInput, LearningOutcome};

    struct Transport {
        profile: ModelProfile,
        response: LearnerInferenceResponse,
        calls: Arc<AtomicUsize>,
    }

    impl LearnerInferenceTransport for Transport {
        fn profile(&self) -> Result<ModelProfile, AgentFailure> {
            Ok(self.profile.clone())
        }

        async fn generate(
            &self,
            _route: PlannedRoute,
            _request: LearnerModelRequest,
        ) -> Result<LearnerInferenceResponse, AgentFailure> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.response.clone())
        }
    }

    fn profile(
        execution_location: ExecutionLocation,
        data_recipient: DataRecipient,
    ) -> ModelProfile {
        ModelProfile {
            id: "learner-profile".into(),
            purpose: ModelPurpose::new(LEARNER_INFERENCE_PURPOSE).unwrap(),
            consumer: ModelConsumer::new(LEARNER_INFERENCE_CONSUMER).unwrap(),
            execution_location,
            data_recipient,
            capabilities: ModelCapabilities(vec![LEARNER_INFERENCE_CAPABILITY.into()]),
            available: true,
        }
    }

    fn request() -> LearnerModelRequest {
        LearnerModelRequest {
            input: LearnerReviewInput {
                schema_version: KNOWLEDGE_VERSION,
                run_id: Uuid::new_v4(),
                person_id: PersonId::new(),
                session_id: Uuid::new_v4(),
                session_revision: 1,
                turn_ids: vec![Uuid::new_v4()],
                outcome: LearningOutcome::Completed,
                digest: "The user asked Floe to remember a preference.".into(),
                current_memories: vec![],
                observed_at: Utc::now(),
            },
            remaining_tokens: 8_192,
            remaining_cost_micros: 50_000,
            max_output_bytes: 4 * 1024,
            deadline: Instant::now() + Duration::from_secs(1),
            cancellation: floe_execution::Cancellation::default(),
        }
    }

    fn make_transport(profile: ModelProfile) -> (Transport, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Transport {
                profile,
                response: LearnerInferenceResponse {
                    text: format!(r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null}}"#),
                    used_tokens: 10,
                    cost_micros: 2,
                },
                calls: calls.clone(),
            },
            calls,
        )
    }

    #[tokio::test]
    async fn device_route_reaches_transport_and_parses_strict_output() {
        let (transport, calls) =
            make_transport(profile(ExecutionLocation::Device, DataRecipient::Device));
        let model = InferenceLearnerModel::new(transport);

        let output = model.review(request()).await.unwrap();

        assert_eq!(output.schema_version, KNOWLEDGE_VERSION);
        assert_eq!(output.proposal, None);
        assert_eq!(output.used_tokens, 10);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn gateway_or_external_routes_are_denied_before_generation() {
        let mut unavailable = profile(ExecutionLocation::Device, DataRecipient::Device);
        unavailable.available = false;
        let (transport, calls) = make_transport(unavailable);
        assert_eq!(
            InferenceLearnerModel::new(transport)
                .review(request())
                .await,
            Err(AgentFailure::ModelUnavailable)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        for (execution_location, data_recipient) in [
            (ExecutionLocation::Gateway, DataRecipient::Device),
            (
                ExecutionLocation::Remote,
                DataRecipient::external("provider").unwrap(),
            ),
        ] {
            let (transport, calls) = make_transport(profile(execution_location, data_recipient));
            let model = InferenceLearnerModel::new(transport);

            assert_eq!(
                model.review(request()).await,
                Err(AgentFailure::PolicyDenied)
            );
            assert_eq!(calls.load(Ordering::Relaxed), 0);
        }
    }

    #[tokio::test]
    async fn malformed_output_and_usage_overruns_fail_closed() {
        for text in [
            r#"{"schema_version":1,"schema_version":1,"proposal":null}"#,
            r#"{"schema_version":1,"proposal":null,"proposal":null}"#,
        ] {
            let (mut transport, _) =
                make_transport(profile(ExecutionLocation::Device, DataRecipient::Device));
            transport.response.text = text.into();
            assert_eq!(
                InferenceLearnerModel::new(transport)
                    .review(request())
                    .await,
                Err(AgentFailure::InvalidModelOutput)
            );
        }
        let (mut transport, calls) =
            make_transport(profile(ExecutionLocation::Device, DataRecipient::Device));
        transport.response.text = r#"{"schema_version":1,"proposal":null,"extra":true}"#.into();
        let model = InferenceLearnerModel::new(transport);
        assert_eq!(
            model.review(request()).await,
            Err(AgentFailure::InvalidModelOutput)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        let (mut transport, calls) =
            make_transport(profile(ExecutionLocation::Device, DataRecipient::Device));
        transport.response.text = r#"{"schema_version":1}"#.into();
        let model = InferenceLearnerModel::new(transport);
        assert_eq!(
            model.review(request()).await,
            Err(AgentFailure::InvalidModelOutput)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        let (mut transport, calls) =
            make_transport(profile(ExecutionLocation::Device, DataRecipient::Device));
        transport.response.used_tokens = 9_000;
        let model = InferenceLearnerModel::new(transport);
        assert_eq!(
            model.review(request()).await,
            Err(AgentFailure::BudgetExceeded)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
