package integrations

import (
 "context"
 "floe/server/internal/connections"
 "floe/server/internal/trust"
 "floe/server/internal/views"
)

type Trust interface { WithPairingOperation(trust.OperatorPrincipal,string,string,string,func(trust.PrincipalSnapshot)error)error; WithCurrentPrincipal(trust.Principal,func(trust.PrincipalSnapshot)error)error; PendingCleanup()([]trust.CleanupTicket,error); AcknowledgeCleanup(context.Context,trust.CleanupTicket,trust.CleanupReceipt)error; ProducerMetadata()(trust.ProducerMetadata,error); SignProducerChallenge([]byte)([]byte,error); RequiredSecurityError()error }
type CredentialBinding struct { Slot,ConnectionID,PersonID,Incarnation string; Generation uint64 }
type AttemptRef struct { ID,ConnectionID string; BindingGeneration uint64 }
type AuthorizationState string
const (AwaitingUser AuthorizationState="awaiting_user";Cancelled AuthorizationState="cancelled";Pending AuthorizationState="pending";Connected AuthorizationState="connected";Disconnected AuthorizationState="disconnected";Failed AuthorizationState="failed")
type AuthorizationProgress struct { Attempt AttemptRef; State AuthorizationState; AuthorizationURL,UserCode,ErrorCode string }
type CredentialStatus struct { Ready,IdentityVerified bool; ReasonCode string }
type ProviderIdentity struct { Namespace,Subject string; Verified bool; Generation uint64 }
type Setup interface { Begin(context.Context,CredentialBinding)(AuthorizationProgress,error); Poll(context.Context,AttemptRef)(AuthorizationProgress,error); Cancel(context.Context,AttemptRef)error; Disconnect(context.Context,CredentialBinding)error; CachedStatus(CredentialBinding)CredentialStatus }
type IdentityVerifier interface { Preflight(context.Context,CredentialBinding)(ProviderIdentity,error); WithVerified(CredentialBinding,ProviderIdentity,func()error)error }
type RuntimeConfig struct { Record connections.Record; Binding CredentialBinding }
type RuntimeFactory interface { Open(context.Context,RuntimeConfig)(Runtime,error) }
type FactoryFunc func(context.Context,RuntimeConfig)(Runtime,error)
func(f FactoryFunc)Open(c context.Context,r RuntimeConfig)(Runtime,error){return f(c,r)}
// Read ports are bounded S2 adapters. Every runtime is connection-scoped and owns its immutable credential binding.
type Runtime struct { Descriptor Descriptor;Readers map[views.ID]views.Reader; Setup Setup; Identity IdentityVerifier; IdentitySupported bool; Snapshot connections.SnapshotSource; Calendar connections.CalendarRuntime; Communication connections.CommunicationRuntime; Work connections.WorkContextRuntime; Logistics connections.LogisticsRuntime; Close func() }
