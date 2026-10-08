use floe_a2a::{
    A2A_EXCHANGE_CONTRACT_VERSION, A2aArtifact, A2aArtifactPart, A2aEnvelope, A2aFailure,
    A2aMessage, A2aPeerAgentId, A2aPeerContextId, A2aPeerId, A2aPeerMessageId, A2aProtocolPolicy,
    MAX_A2A_ARTIFACT_BYTES, MAX_A2A_ARTIFACT_PARTS, MAX_A2A_ENVELOPE_BYTES, MAX_A2A_MESSAGE_BYTES,
    MAX_A2A_TOTAL_ARTIFACT_BYTES,
};

fn policy() -> A2aProtocolPolicy {
    A2aProtocolPolicy::new(["urn:floe:test:context:v1".to_owned()]).expect("valid extension policy")
}

fn envelope() -> A2aEnvelope {
    A2aEnvelope {
        contract_version: A2A_EXCHANGE_CONTRACT_VERSION,
        extensions: vec!["urn:floe:test:context:v1".into()],
        message: A2aMessage {
            peer_id: A2aPeerId::try_new("peer-a").expect("peer ID"),
            sender_agent_id: A2aPeerAgentId::try_new("agent-a").expect("agent ID"),
            context_id: A2aPeerContextId::try_new("context-a").expect("context ID"),
            message_id: A2aPeerMessageId::try_new("message-a").expect("message ID"),
            task_id: None,
            content: "Review this request.".into(),
            artifacts: Vec::new(),
        },
    }
}

fn artifact(artifact_id: String, data_len: usize) -> A2aArtifact {
    A2aArtifact {
        artifact_id,
        name: "payload".into(),
        parts: vec![A2aArtifactPart::Data {
            media_type: "application/json".into(),
            data: "x".repeat(data_len),
        }],
    }
}

#[test]
fn utf8_message_and_artifact_byte_limits_are_inclusive() {
    assert_eq!(
        A2aPeerId::try_new("peer\nname"),
        Err(A2aFailure::InvalidEnvelope)
    );
    let mut message = envelope();
    let mut content = "한".repeat(MAX_A2A_MESSAGE_BYTES / 3);
    content.push_str(&"x".repeat(MAX_A2A_MESSAGE_BYTES % 3));
    assert_eq!(content.len(), MAX_A2A_MESSAGE_BYTES);
    message.message.content = content;
    message
        .validate(&policy())
        .expect("UTF-8 byte boundary accepted");
    message.message.content.push('x');
    assert_eq!(
        message.validate(&policy()),
        Err(A2aFailure::InvalidEnvelope)
    );

    let mut artifact_message = envelope();
    let mut text = "한".repeat(MAX_A2A_ARTIFACT_BYTES / 3);
    text.push_str(&"x".repeat(MAX_A2A_ARTIFACT_BYTES % 3));
    assert_eq!(text.len(), MAX_A2A_ARTIFACT_BYTES);
    artifact_message.message.artifacts = vec![A2aArtifact {
        artifact_id: "unicode-artifact".into(),
        name: "Korean text".into(),
        parts: vec![A2aArtifactPart::Text { text }],
    }];
    artifact_message
        .validate(&policy())
        .expect("UTF-8 artifact byte boundary accepted");
    let A2aArtifactPart::Text { text } = &mut artifact_message.message.artifacts[0].parts[0] else {
        unreachable!();
    };
    text.push('x');
    assert_eq!(
        artifact_message.validate(&policy()),
        Err(A2aFailure::InvalidEnvelope)
    );
}

#[test]
fn artifact_part_count_and_total_payload_bytes_are_bounded() {
    let mut exact_parts = envelope();
    exact_parts.message.artifacts = vec![A2aArtifact {
        artifact_id: "many-parts".into(),
        name: "parts".into(),
        parts: vec![A2aArtifactPart::Text { text: "x".into() }; MAX_A2A_ARTIFACT_PARTS],
    }];
    exact_parts
        .validate(&policy())
        .expect("exact part count boundary accepted");
    exact_parts.message.artifacts[0]
        .parts
        .push(A2aArtifactPart::Text { text: "x".into() });
    assert_eq!(
        exact_parts.validate(&policy()),
        Err(A2aFailure::InvalidEnvelope)
    );

    let specifications = (0..4)
        .map(|index| {
            (
                format!("artifact-{index}"),
                "payload".to_owned(),
                "application/json".to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let metadata_bytes = specifications
        .iter()
        .map(|(id, name, media_type)| id.len() + name.len() + media_type.len())
        .sum::<usize>();
    let data_bytes = MAX_A2A_TOTAL_ARTIFACT_BYTES - metadata_bytes;
    let per_part = data_bytes / specifications.len();
    let remainder = data_bytes % specifications.len();
    let mut exact_aggregate = envelope();
    exact_aggregate.message.artifacts = specifications
        .into_iter()
        .enumerate()
        .map(|(index, (artifact_id, name, media_type))| A2aArtifact {
            artifact_id,
            name,
            parts: vec![A2aArtifactPart::Data {
                media_type,
                data: "x".repeat(per_part + usize::from(index < remainder)),
            }],
        })
        .collect();
    exact_aggregate
        .validate(&policy())
        .expect("exact aggregate artifact byte boundary accepted");
    let A2aArtifactPart::Data { data, .. } = &mut exact_aggregate.message.artifacts[0].parts[0]
    else {
        unreachable!();
    };
    data.push('x');
    assert_eq!(
        exact_aggregate.validate(&policy()),
        Err(A2aFailure::InvalidEnvelope)
    );

    let mut duplicate_identity = envelope();
    duplicate_identity.message.artifacts =
        vec![artifact("same-id".into(), 1), artifact("same-id".into(), 1)];
    assert_eq!(
        duplicate_identity.validate(&policy()),
        Err(A2aFailure::InvalidEnvelope)
    );
}

#[test]
fn serialized_envelope_size_boundary_is_inclusive() {
    let part_count = 5usize;
    let mut exact = envelope();
    exact.message.artifacts = vec![A2aArtifact {
        artifact_id: "framing-boundary".into(),
        name: "escaped data".into(),
        parts: (0..part_count)
            .map(|_| A2aArtifactPart::Data {
                media_type: "application/json".into(),
                data: String::new(),
            })
            .collect(),
    }];
    let base_size = serde_json::to_vec(&exact)
        .expect("encode empty data framing")
        .len();
    let extra_size = MAX_A2A_ENVELOPE_BYTES - base_size;
    let x_count = if extra_size % 2 == 0 { 6 } else { 5 };
    let escaped_quote_count = (extra_size - x_count) / 2;
    let quotes_per_part = escaped_quote_count / part_count;
    let quote_remainder = escaped_quote_count % part_count;
    let extra_x_count = x_count - part_count;
    for (index, part) in exact.message.artifacts[0].parts.iter_mut().enumerate() {
        let A2aArtifactPart::Data { data, .. } = part else {
            unreachable!();
        };
        data.push_str(&"\"".repeat(quotes_per_part + usize::from(index < quote_remainder)));
        data.push('x');
        if index < extra_x_count {
            data.push('x');
        }
    }
    assert_eq!(
        serde_json::to_vec(&exact)
            .expect("encode exact envelope")
            .len(),
        MAX_A2A_ENVELOPE_BYTES
    );
    exact
        .validate(&policy())
        .expect("exact serialized envelope boundary accepted");

    let A2aArtifactPart::Data { data, .. } = &mut exact.message.artifacts[0].parts[0] else {
        unreachable!();
    };
    data.push('x');
    assert_eq!(
        serde_json::to_vec(&exact)
            .expect("encode over-limit envelope")
            .len(),
        MAX_A2A_ENVELOPE_BYTES + 1
    );
    assert_eq!(exact.validate(&policy()), Err(A2aFailure::InvalidEnvelope));
}
