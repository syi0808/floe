use std::sync::Arc;
use floe_access::{GatewayTrustReader,SourcePreviewVerifier,SignedSourcePreview,RemotePairingIdentity,RemoteSourceQuery,RemoteViewSourceReference,SourceAuthority};
use floe_agent_contract::{AgentFailure,BoxFuture};
use ring::signature;
use uuid::Uuid;
use super::proof::*;
use super::json::strict_json_bytes;
const MAX_SOURCE_PREVIEW_PROOF_BYTES:usize=16384;
const PRODUCER_SIGNATURE_DOMAIN:&[u8]=b"floe.remote.producer.v1\0";
pub struct GatewaySourcePreviewVerifier {trust:Arc<dyn GatewayTrustReader>}
impl GatewaySourcePreviewVerifier {pub fn new(trust:Arc<dyn GatewayTrustReader>)->Self{Self{trust}}}
impl SourcePreviewVerifier for GatewaySourcePreviewVerifier {
    fn verify<'a>(&'a self,preview:&'a SignedSourcePreview,pairing:RemotePairingIdentity<'a>,query:RemoteSourceQuery<'a>)->BoxFuture<'a,Result<RemoteViewSourceReference,AgentFailure>>{
        Box::pin(async move {
        let descriptor_b64url=&preview.descriptor_b64url;let producer_signature_b64url=&preview.producer_signature;
        let person_id=pairing.person_id;let client_id=pairing.client_id;let device_id=pairing.device_id;
        let view_id=query.view_id;let connector_id=query.connector_id;let connection_id=query.connection_id;let resource=query.resource;
        let descriptor = decode_canonical(descriptor_b64url, MAX_SOURCE_PREVIEW_PROOF_BYTES)?;
        let producer_signature = decode_exact(producer_signature_b64url, 64)?;
        strict_json_bytes(&descriptor, MAX_SOURCE_PREVIEW_PROOF_BYTES)?;
        let wire: ViewSourcePreviewWire =
            serde_json::from_slice(&descriptor).map_err(|_| AgentFailure::InvalidInput)?;
        let producer = self.trust.pinned_producer().await?;
        validate_producer(&producer)?;
        if producer != preview.producer { return Err(AgentFailure::PolicyDenied); }
        let producer_key = decode_exact(&producer.public_key, 32)?;
        let mut message = Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + descriptor.len());
        message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
        message.extend_from_slice(&descriptor);
        signature::UnparsedPublicKey::new(&signature::ED25519, producer_key)
            .verify(&message, &producer_signature)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if wire.v != 1
            || wire.operation != "remote_view_source_preview"
            || wire.view_id != view_id
            || wire.person_id != person_id
            || wire.client_id != client_id
            || wire.device_id != device_id
            || wire.audience != producer.audience
            || wire.connector_id != connector_id
            || wire.connection_id != connection_id
            || wire.resource != resource
            || wire.source_resources.is_empty()
            || wire.source_resources.windows(2).any(|pair| pair[0] >= pair[1])
            || wire.execution_owner != producer.execution_owner
            || !valid_text(&wire.nonce, 256)
            || wire.connection_revision == 0
            || wire.connection_revision != preview.connection_revision
            || !valid_text(&wire.provider_identity, 256)
            || wire.issued_at_unix_ms <= chrono::Utc::now().timestamp_millis().saturating_sub(300_000)
            || wire.issued_at_unix_ms > chrono::Utc::now().timestamp_millis().saturating_add(5_000)
            || Uuid::parse_str(&wire.challenge_id).is_err()
            || Uuid::parse_str(&wire.incarnation).is_err()
            || Uuid::parse_str(&wire.incarnation).is_ok_and(|identifier| identifier.is_nil())
            || wire.epoch == 0
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let source_authority = SourceAuthority::from_parts(
            Uuid::parse_str(&wire.incarnation).map_err(|_| AgentFailure::PolicyDenied)?,
            std::num::NonZeroU64::new(wire.epoch).ok_or(AgentFailure::PolicyDenied)?,
        )
        .ok_or(AgentFailure::PolicyDenied)?;
        let source_resources = wire
            .source_resources
            .into_iter()
            .map(|resource| floe_access::ResourceHandle::try_new(resource).map_err(|_| AgentFailure::PolicyDenied))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RemoteViewSourceReference {
            view_id: wire.view_id,
            person_id: wire.person_id,
            client_id: wire.client_id,
            device_id: wire.device_id,
            audience: wire.audience,
            connector_id: wire.connector_id,
            connection_id: wire.connection_id,
            connection_revision: wire.connection_revision,
            execution_owner: wire.execution_owner,
            source_authority,
            resource: wire.resource,
            source_resources,
            provider_identity: wire.provider_identity,
        })

        })
    }
}
