use floe_a2a::{
    A2A_EXCHANGE_CONTRACT_VERSION, A2aArtifactPart, A2aEnvelope, A2aFailure, A2aInboundMapping,
    A2aMessage, A2aPeerAgentId, A2aPeerContextId, A2aPeerId, A2aPeerMessageId, A2aPeerTaskId,
    A2aProtocolPolicy, A2aTaskObservation, AuthenticatedPeerAgent, HostedTaskAdmission,
    map_inbound_message,
};
use floe_conversation_contract::{
    AdmissionTarget, AgentIdentity, AgentInstanceId, AssignmentId, ConversationBranchId,
    ConversationId, MessageId, MessageOrigin,
};
use floe_kernel::{CommandId, PersonId, TaskId};
use uuid::Uuid;

fn uuid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn agent_identity(agent: u128, assignment: u128) -> AgentIdentity {
    AgentIdentity {
        person_id: PersonId::from_uuid(uuid(10)).expect("person"),
        agent_instance_id: AgentInstanceId::from_uuid(uuid(agent)).expect("agent"),
        assignment_id: AssignmentId::from_uuid(uuid(assignment)).expect("assignment"),
        definition_id: format!("expert.{agent}"),
        definition_revision: 1,
    }
}

fn peer(value: &str) -> A2aPeerId {
    A2aPeerId::try_new(value).expect("peer ID")
}

fn envelope(peer_id: &str) -> A2aEnvelope {
    A2aEnvelope {
        contract_version: A2A_EXCHANGE_CONTRACT_VERSION,
        extensions: vec!["urn:floe:test:context:v1".into()],
        message: A2aMessage {
            peer_id: peer(peer_id),
            sender_agent_id: A2aPeerAgentId::try_new("manager-installed-instance")
                .expect("remote agent ID"),
            context_id: A2aPeerContextId::try_new("same-remote-context")
                .expect("remote context ID"),
            message_id: A2aPeerMessageId::try_new("same-remote-message")
                .expect("remote message ID"),
            task_id: Some(A2aPeerTaskId::try_new("remote-task-7").expect("remote Task ID")),
            content: "Please review this schedule constraint.".into(),
            artifacts: vec![floe_a2a::A2aArtifact {
                artifact_id: "artifact-1".into(),
                name: "review-note".into(),
                parts: vec![floe_a2a::A2aArtifactPart::Text {
                    text: "Consider moving this appointment.".into(),
                }],
            }],
        },
    }
}

fn mapping(peer_id: &str, host_agent: u128, conv_id: u128) -> A2aInboundMapping {
    A2aInboundMapping {
        peer_id: peer(peer_id),
        remote_context_id: A2aPeerContextId::try_new("same-remote-context")
            .expect("remote context ID"),
        remote_message_id: A2aPeerMessageId::try_new("same-remote-message")
            .expect("remote message ID"),
        remote_task_id: Some(A2aPeerTaskId::try_new("remote-task-7").expect("remote Task ID")),
        host_agent_instance_id: AgentInstanceId::from_uuid(uuid(host_agent)).expect("host agent"),
        local_target: AdmissionTarget::New {
            conversation_id: ConversationId::from_uuid(uuid(conv_id)).expect("conversation"),
            branch_id: ConversationBranchId::from_uuid(uuid(conv_id + 1)).expect("branch"),
            identity: agent_identity(host_agent, host_agent + 10),
        },
        local_message_id: MessageId::from_uuid(uuid(conv_id + 2)).expect("local message"),
        local_command_id: CommandId::from_uuid(uuid(conv_id + 3)).expect("local command"),
        local_task_id: Some(TaskId::from_uuid(uuid(conv_id + 4)).expect("host Task ID")),
    }
}

#[test]
fn protocol_version_and_extensions_are_explicitly_validated() {
    let policy = A2aProtocolPolicy::new(["urn:floe:test:context:v1".to_owned()])
        .expect("valid extension policy");
    let request = envelope("peer-a");
    request
        .validate(&policy)
        .expect("known version and extension");

    let mut unsupported_version = request.clone();
    unsupported_version.contract_version += 1;
    assert_eq!(
        unsupported_version.validate(&policy),
        Err(A2aFailure::UnsupportedVersion)
    );

    let mut unsupported_extension = request;
    unsupported_extension.extensions = vec!["urn:unknown:extension".into()];
    assert_eq!(
        unsupported_extension.validate(&policy),
        Err(A2aFailure::UnsupportedExtension)
    );

    let mut too_many_extensions = envelope("peer-a");
    too_many_extensions.extensions = (0..=floe_a2a::MAX_A2A_EXTENSIONS)
        .map(|index| format!("urn:floe:test:extension:{index}"))
        .collect();
    assert_eq!(
        too_many_extensions.validate(&policy),
        Err(A2aFailure::InvalidEnvelope)
    );
}

#[test]
fn same_remote_context_maps_to_isolated_local_conversations_per_peer() {
    let policy = A2aProtocolPolicy::new(["urn:floe:test:context:v1".to_owned()])
        .expect("valid extension policy");
    let first = envelope("peer-a");
    let first_peer = AuthenticatedPeerAgent::from_verified_binding(
        peer("peer-a"),
        first.message.sender_agent_id.clone(),
        AgentInstanceId::from_uuid(uuid(30)).expect("Manager instance"),
    )
    .expect("verified peer binding");
    let first_mapping = mapping("peer-a", 20, 100);
    let first_request = map_inbound_message(&first, &policy, &first_peer, &first_mapping)
        .expect("map remote context through the peer binding");

    let second = envelope("peer-b");
    let second_peer = AuthenticatedPeerAgent::from_verified_binding(
        peer("peer-b"),
        second.message.sender_agent_id.clone(),
        AgentInstanceId::from_uuid(uuid(31)).expect("second Manager instance"),
    )
    .expect("second verified peer binding");
    let second_mapping = mapping("peer-b", 21, 200);
    let second_request = map_inbound_message(&second, &policy, &second_peer, &second_mapping)
        .expect("map the same remote context name independently");

    let AdmissionTarget::New {
        conversation_id: first_conversation,
        ..
    } = first_request.message.target
    else {
        panic!("first remote objective starts a new conversation");
    };
    let AdmissionTarget::New {
        conversation_id: second_conversation,
        ..
    } = second_request.message.target
    else {
        panic!("second remote objective starts a new conversation");
    };
    assert_ne!(first_conversation, second_conversation);
    assert_eq!(
        first_request.message.message.origin,
        MessageOrigin::Agent {
            agent_instance_id: first_peer.agent_instance_id()
        }
    );
    assert_eq!(
        second_request.message.message.origin,
        MessageOrigin::Agent {
            agent_instance_id: second_peer.agent_instance_id()
        }
    );
    assert_ne!(
        first_request.message.message.message_id,
        second_request.message.message.message_id
    );
    assert_ne!(
        first_request.message.message.task_id,
        second_request.message.message.task_id
    );
    assert_eq!(first_request.artifacts, first.message.artifacts);
    assert_eq!(second_request.artifacts, second.message.artifacts);
}

#[test]
fn mismatched_authenticated_peer_or_local_agent_mapping_is_rejected() {
    let policy = A2aProtocolPolicy::new(["urn:floe:test:context:v1".to_owned()])
        .expect("valid extension policy");
    let request = envelope("peer-a");
    let verified_peer = AuthenticatedPeerAgent::from_verified_binding(
        peer("peer-b"),
        request.message.sender_agent_id.clone(),
        AgentInstanceId::from_uuid(uuid(30)).expect("Manager instance"),
    )
    .expect("verified binding for another peer");
    assert_eq!(
        map_inbound_message(
            &request,
            &policy,
            &verified_peer,
            &mapping("peer-a", 20, 100)
        ),
        Err(A2aFailure::InvalidPeerBinding)
    );

    let peer = AuthenticatedPeerAgent::from_verified_binding(
        peer("peer-a"),
        request.message.sender_agent_id.clone(),
        AgentInstanceId::from_uuid(uuid(30)).expect("Manager instance"),
    )
    .expect("verified binding");
    let mut mismatched_mapping = mapping("peer-a", 20, 100);
    mismatched_mapping.host_agent_instance_id =
        AgentInstanceId::from_uuid(uuid(21)).expect("different mapped host agent");
    assert_eq!(
        map_inbound_message(&request, &policy, &peer, &mismatched_mapping),
        Err(A2aFailure::MappingMismatch)
    );
}

#[test]
fn host_task_admission_rechecks_the_stored_evidence_commitment() {
    let policy = A2aProtocolPolicy::new(["urn:floe:test:context:v1".to_owned()])
        .expect("valid extension policy");
    let request = envelope("peer-a");
    let peer = AuthenticatedPeerAgent::from_verified_binding(
        request.message.peer_id.clone(),
        request.message.sender_agent_id.clone(),
        AgentInstanceId::from_uuid(uuid(30)).expect("host agent instance"),
    )
    .expect("verified peer mapping");
    let mapped = map_inbound_message(&request, &policy, &peer, &mapping("peer-a", 20, 100))
        .expect("map message and evidence");
    mapped.validate().expect("mapped evidence is committed");

    let mut hosted = HostedTaskAdmission {
        message: mapped.message,
        artifacts: mapped.artifacts,
    };
    hosted
        .validate()
        .expect("host port sees a matching evidence reference");
    let A2aArtifactPart::Text { text } = &mut hosted.artifacts[0].parts[0] else {
        panic!("fixture's first artifact part is text");
    };
    text.push_str(" mutated before persistence");
    assert_eq!(hosted.validate(), Err(A2aFailure::MappingMismatch));
}

#[test]
fn remote_task_observation_keeps_external_status_and_id_separate() {
    let observation = A2aTaskObservation {
        peer_id: peer("peer-a"),
        context_id: A2aPeerContextId::try_new("remote-context-9").expect("remote context"),
        task_id: A2aPeerTaskId::try_new("task:9").expect("remote Task"),
        status_code: "completed".into(),
        artifacts: vec![],
    };
    observation.validate().expect("valid external observation");
    assert_eq!(observation.status_code, "completed");
    assert_eq!(observation.task_id.as_str(), "task:9");
    // The A2A observation has no conversion into a host TaskReceipt; only the
    // host Task port returns that immutable, host-owned value.
}

#[test]
fn mapping_rejects_malformed_deserialized_target_values() {
    let policy = A2aProtocolPolicy::new(["urn:floe:test:context:v1".to_owned()])
        .expect("valid extension policy");
    let request = envelope("peer-a");
    let peer = AuthenticatedPeerAgent::from_verified_binding(
        peer("peer-a"),
        request.message.sender_agent_id.clone(),
        AgentInstanceId::from_uuid(uuid(30)).expect("host agent instance"),
    )
    .expect("verified peer mapping");

    let mut nil_conversation = mapping("peer-a", 20, 100);
    let mut target = serde_json::to_value(&nil_conversation.local_target).expect("target JSON");
    target["conversation_id"] = serde_json::json!(Uuid::nil().to_string());
    nil_conversation.local_target =
        serde_json::from_value(target).expect("deserialize a nil wrapped ID");
    assert_eq!(
        map_inbound_message(&request, &policy, &peer, &nil_conversation),
        Err(A2aFailure::MappingMismatch)
    );

    let mut nil_branch = mapping("peer-a", 20, 100);
    let mut target = serde_json::to_value(&nil_branch.local_target).expect("target JSON");
    target["branch_id"] = serde_json::json!(Uuid::nil().to_string());
    nil_branch.local_target = serde_json::from_value(target).expect("deserialize a nil branch ID");
    assert_eq!(
        map_inbound_message(&request, &policy, &peer, &nil_branch),
        Err(A2aFailure::MappingMismatch)
    );

    let mut nil_message_id = mapping("peer-a", 20, 100);
    nil_message_id.local_message_id =
        serde_json::from_value(serde_json::json!(Uuid::nil().to_string()))
            .expect("deserialize a nil local Message ID");
    assert_eq!(
        map_inbound_message(&request, &policy, &peer, &nil_message_id),
        Err(A2aFailure::MappingMismatch)
    );

    let mut zero_head = mapping("peer-a", 20, 100);
    let continue_json = serde_json::json!({
        "kind": "continue",
        "reference": {
            "conversation_id": uuid(100).to_string(),
            "branch_id": uuid(101).to_string(),
            "identity": serde_json::to_value(agent_identity(20, 30)).expect("identity JSON"),
            "head_revision": 0
        }
    });
    zero_head.local_target =
        serde_json::from_value(continue_json).expect("deserialize a zero-head continuation target");
    assert_eq!(
        map_inbound_message(&request, &policy, &peer, &zero_head),
        Err(A2aFailure::MappingMismatch)
    );
}
