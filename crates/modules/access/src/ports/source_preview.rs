use crate::{
    RemotePairingIdentity, RemoteSourceQuery, RemoteViewSourceReference, SignedSourcePreview,
};
use floe_execution::BoxFuture;
use floe_kernel::AgentFailure;
/// Cryptographic signed-descriptor verification at the real Gateway boundary.
pub trait SourcePreviewVerifier: Send + Sync {
    fn verify<'a>(
        &'a self,
        preview: &'a SignedSourcePreview,
        pairing: RemotePairingIdentity<'a>,
        query: RemoteSourceQuery<'a>,
    ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>>;
}
