// Package pairing owns bounded temporary enrollment; trust owns the durable commit.
package pairing

import (
 "context"
 "sync"
 "time"
 "floe/server/internal/trust"
 "floe/server/internal/credentials"
)

type Trust interface {
 PreparePairing(string)(uint64,error)
 ReadPairing(context.Context,string,string)(trust.PairingReadback,error)
 ProducerMetadata()(trust.ProducerMetadata,error)
 SignProducerChallenge([]byte)([]byte,error)
 ActivatePairing(context.Context,trust.PairingActivation)(trust.PairingCommit,error)
 WithCurrentOperator(trust.OperatorPrincipal,func()error)error
}
type Operations struct { mu sync.Mutex; pending *Pending; lastPair time.Time; clock func()time.Time; trust Trust;credentials credentials.Store }
func NewOperations(service Trust,store credentials.Store,clock func()time.Time)*Operations{if clock==nil{clock=time.Now};return &Operations{trust:service,clock:clock,credentials:store}}
func(o *Operations) ClearClient(id string){if o.credentials!=nil{_ = o.credentials.Delete("FLOE_PAIRING_"+id)};o.mu.Lock();defer o.mu.Unlock();if o.pending!=nil&&o.pending.ID==id{o.pending=nil}}
func(o *Operations) Pending()*Pending{o.mu.Lock();defer o.mu.Unlock();index,err:=o.index();if err!=nil{return nil};for i:=len(index.Entries)-1;i>=0;i--{p,err:=o.load(index.Entries[i]);if err!=nil{return nil};if p.AdminApproved||p.status=="rejected"||p.status=="cancelled"||!p.Expires.After(o.clock()){continue};o.pending=p;copy:=*p;copy.challengeBytes=nil;copy.producerSignature=nil;copy.localProof=trust.Proof{};copy.proof="";copy.token="";return &copy};return nil}

type Pending struct {
 ID string `json:"id"`
 Code string `json:"code"`
 Expires time.Time `json:"expires"`
 PersonID string `json:"person_id"`
 DeviceID string `json:"device_id"`
 IssuerKeyID string `json:"issuer_key_id"`
 IssuerPublicKey string `json:"issuer_public_key"`
 IssuerFingerprint string `json:"issuer_fingerprint"`
 ProducerFingerprint string `json:"producer_fingerprint"`
 ProducerAudience string `json:"producer_audience"`
 LocalConfirmed bool `json:"local_confirmed"`
 AdminApproved bool `json:"admin_approved"`
 status,challengeID,challengeB64,proof,token string
 challengeBytes,producerSignature []byte
 localProof trust.Proof
 producer trust.ProducerMetadata
 expectedRevision uint64
 operationID string
}
type Request struct {
 SchemaVersion int `json:"schema_version"`
 OperationID string `json:"operation_id"`
 PairingID string `json:"pairing_id"`
 Proof string `json:"proof"`
 PersonID string `json:"person_id"`
 DeviceID string `json:"device_id"`
 IssuerKeyID string `json:"issuer_key_id"`
 IssuerPublicKey string `json:"issuer_public_key"`
 ChallengeID string `json:"challenge_id"`
 KeyID string `json:"key_id"`
 Signature string `json:"signature"`
}
type ApprovalRequest struct { SchemaVersion int `json:"schema_version"`; PairingID string `json:"pairing_id"`; Fingerprint string `json:"issuer_fingerprint"` }
type RejectionRequest struct { SchemaVersion int `json:"schema_version"`; PairingID string `json:"pairing_id"` }
