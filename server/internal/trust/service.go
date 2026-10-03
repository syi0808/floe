// Package trust is the only issuer of authenticated client and operator capabilities.
package trust

import (
 "context"
 "crypto/ed25519"
 "crypto/subtle"
 "encoding/base64"
 "encoding/hex"
 "encoding/json"
 "errors"
 "os"
 "path/filepath"
 "sort"
 "sync"
 "time"

 "floe/server/internal/operation"
 "floe/server/internal/storage"
)

// Principal is immutable, process-bound, and stamped with a shared trust generation.
// Its zero value and values decoded from JSON convey no authority.
type Principal struct { client,person,device string; generation uint64; owner *Service }
func(p Principal) ClientID() string{return p.client}
func(p Principal) PersonID() string{return p.person}
func(p Principal) DeviceID() string{return p.device}
func(p Principal) Valid() bool{return p.owner!=nil&&p.generation!=0&&p.client!=""}
func(p Principal) Same(other Principal) bool{return p==other&&p.Valid()}
type PrincipalSnapshot struct { ClientID,PersonID,DeviceID string; Generation uint64; Active,CleanupBlocked bool }
type IssuerSnapshot struct { Principal Principal; KeyID,EnrollmentID string; PublicKey ed25519.PublicKey; Generation uint64 }
type ProducerMetadata struct { SchemaVersion int `json:"schema_version"`; InstanceID string `json:"instance_id"`; ExecutionOwner string `json:"execution_owner"`; Audience string `json:"audience"`; KeyID string `json:"key_id"`; PublicKey string `json:"public_key"`; Fingerprint string `json:"fingerprint"` }
type PairingActivation struct {
 PairingID, PersonID, DeviceID string
 Producer ProducerMetadata
 IssuerKeyID, IssuerFingerprint string
 IssuerPublicKey ed25519.PublicKey
 ChallengeID string
 ChallengeBytes []byte
 LocalProof Proof
 AdminFingerprint string
 ExpectedRevision uint64
 TokenHash string
 PollProofHash string
 Operator OperatorPrincipal
}
type PairingCommit struct { PairingID string; Generation uint64 }
type CleanupKind string
const (ClientAttempts CleanupKind="client_attempts"; PersonAllSources CleanupKind="person_all_sources")
type CleanupTicket struct { ID string `json:"id"`; Revision uint64 `json:"revision"`; PersonID string `json:"person_id"`; ClientID string `json:"client_id"`; Kind CleanupKind `json:"kind"`; TrustGeneration uint64 `json:"trust_generation"` }
type CleanupReceipt struct { TicketID string; TicketRevision,TrustGeneration,IntegrationRevision uint64 }
type RevocationReceipt struct { ClientID string; Generation uint64; Cleanup CleanupTicket }
type ClientSnapshot struct { ClientID,PersonID,DeviceID string }
type clientRecord struct { ClientID string `json:"client_id"`; PersonID string `json:"person_id"`; DeviceID string `json:"device_id"`; TokenHash string `json:"token_hash"`; PollProofHash string `json:"poll_proof_hash"` }
type issuerRecord struct { KeyID string `json:"key_id"`; ClientID string `json:"client_id"`; EnrollmentID string `json:"enrollment_id"`; PublicKey []byte `json:"public_key"`; Generation uint64 `json:"generation"` }
type diskState struct { SchemaVersion int `json:"schema_version"`; Revision uint64 `json:"revision"`; InstanceID string `json:"instance_id"`; ExecutionOwner string `json:"execution_owner"`; Clients map[string]clientRecord `json:"clients"`; Issuers map[string]issuerRecord `json:"issuers"`; Revoked map[string]bool `json:"revoked_issuers"`; Cleanup map[string]CleanupTicket `json:"cleanup"` }
type Service struct { mu sync.RWMutex; directory string; state diskState; producer *ProducerIdentity; unavailable bool; operators *operatorSessions }
func fail(category operation.Category,code string) error{return operation.Fail(category,code)}
func Open(directory string)(*Service,error){
 if err:=os.MkdirAll(directory,0700);err!=nil{return nil,fail(operation.Unavailable,"trust_unavailable")};info,err:=os.Lstat(directory);if err!=nil||!info.IsDir()||info.Mode().Perm()&0077!=0{return nil,fail(operation.Unavailable,"trust_unavailable")}
 s:=&Service{directory:directory};path:=filepath.Join(directory,"trust.json")
 data,err:=storage.ReadPrivate(path,1<<20);fresh:=os.IsNotExist(err)
 if fresh {
  // Any partial prior identity/state is a recovery error, never permission to replace it.
  for _,name:=range []string{"state.json","producer-identity.json","admin-token","inference.json","integrations.json"}{if _,e:=os.Lstat(filepath.Join(directory,name));e==nil||!os.IsNotExist(e){return nil,fail(operation.Unavailable,"trust_recovery_required")}}
  s.state=diskState{SchemaVersion:1,Revision:1,InstanceID:NewID(),ExecutionOwner:NewID(),Clients:map[string]clientRecord{},Issuers:map[string]issuerRecord{},Revoked:map[string]bool{},Cleanup:map[string]CleanupTicket{}}
  identity,encoded,e:=GenerateProducerIdentity(NewID());if e!=nil{return nil,e};if e=storage.WritePrivate(filepath.Join(directory,"producer-identity.json"),encoded);e!=nil{return nil,e};s.producer=identity
  if e=storage.WritePrivate(filepath.Join(directory,"admin-token"),[]byte(Token()));e!=nil{return nil,e};if e=s.persist(s.state);e!=nil{return nil,e}
 } else {
  if err!=nil||DecodeStrict(data,&s.state,1<<20,32)!=nil||!validState(s.state){return nil,fail(operation.Unavailable,"trust_unavailable")}
  raw,e:=storage.ReadPrivate(filepath.Join(directory,"producer-identity.json"),4096);if e!=nil{return nil,fail(operation.Unavailable,"producer_unavailable")};s.producer,e=DecodeProducerIdentity(raw);if e!=nil{return nil,fail(operation.Unavailable,"producer_unavailable")}
 }
 admin,err:=storage.ReadPrivate(filepath.Join(directory,"admin-token"),1024);if err!=nil||len(admin)<32{return nil,fail(operation.Unavailable,"operator_unavailable")};s.operators=newOperatorSessions(Digest(string(admin)))
 return s,nil
}
func validState(st diskState)bool{
 if st.SchemaVersion!=1||st.Revision==0||!ValidID(st.InstanceID)||!ValidID(st.ExecutionOwner)||st.Clients==nil||st.Issuers==nil||st.Revoked==nil||st.Cleanup==nil||len(st.Clients)>16||len(st.Issuers)+len(st.Revoked)>128||len(st.Cleanup)>32{return false}
 person:="";bound:=map[string]bool{};hashes:=map[string]bool{}
 for id,c:=range st.Clients{h,e:=hex.DecodeString(c.TokenHash);if id!=c.ClientID||!ValidID(id)||!ValidID(c.PersonID)||!ValidDevice(c.DeviceID)||e!=nil||len(h)!=32||!validDigest(c.PollProofHash)||hex.EncodeToString(h)!=c.TokenHash||hashes[c.TokenHash]||person!=""&&person!=c.PersonID{return false};person=c.PersonID;hashes[c.TokenHash]=true}
 for id,r:=range st.Issuers{_,ok:=st.Clients[r.ClientID];if id!=r.KeyID||!ValidID(id)||!ValidID(r.EnrollmentID)||!ok||bound[r.ClientID]||len(r.PublicKey)!=ed25519.PublicKeySize||st.Revoked[id]||r.Generation==0||r.Generation>st.Revision{return false};bound[r.ClientID]=true}
 for id,v:=range st.Revoked{if !ValidID(id)||!v{return false}}
 for id,t:=range st.Cleanup{if id!=t.ID||!ValidID(id)||!ValidID(t.PersonID)||!ValidID(t.ClientID)||t.Revision==0||t.TrustGeneration==0||t.TrustGeneration>st.Revision||t.Kind!=ClientAttempts&&t.Kind!=PersonAllSources{return false}}
 return true
}
func clone(st diskState)diskState { n:=st;n.Clients=map[string]clientRecord{};n.Issuers=map[string]issuerRecord{};n.Revoked=map[string]bool{};n.Cleanup=map[string]CleanupTicket{};for k,v:=range st.Clients{n.Clients[k]=v};for k,v:=range st.Issuers{v.PublicKey=append([]byte(nil),v.PublicKey...);n.Issuers[k]=v};for k,v:=range st.Revoked{n.Revoked[k]=v};for k,v:=range st.Cleanup{n.Cleanup[k]=v};return n }
func(s *Service) persist(st diskState)error{data,err:=json.Marshal(st);if err!=nil{return fail(operation.Internal,"trust_unavailable")};err=storage.WritePrivate(filepath.Join(s.directory,"trust.json"),data);if storage.IsIndeterminate(err){s.unavailable=true};if err!=nil{return fail(operation.Unavailable,"trust_unavailable")};return nil}
func(s *Service) blocked(person string)bool{for _,t:=range s.state.Cleanup{if t.PersonID==person{return true}};return false}
func(s *Service) RequiredSecurityError()error{s.mu.RLock();defer s.mu.RUnlock();if s.unavailable||s.producer==nil{return fail(operation.Unavailable,"trust_unavailable")};return nil}
func(s *Service) AuthenticateBearer(ctx context.Context,bearer string)(Principal,error){
 if err:=ctx.Err();err!=nil{return Principal{},err};if len(bearer)<32||len(bearer)>256{return Principal{},fail(operation.Unauthenticated,"unauthorized")};hash:=Digest(bearer)
 s.mu.RLock();defer s.mu.RUnlock();if s.unavailable{return Principal{},fail(operation.Unavailable,"trust_unavailable")}
 for _,r:=range s.state.Clients{if subtle.ConstantTimeCompare([]byte(r.TokenHash),[]byte(hash))==1{if s.blocked(r.PersonID){return Principal{},fail(operation.Unavailable,"person_cleanup_pending")};p:=Principal{r.ClientID,r.PersonID,r.DeviceID,s.state.Revision,s};if _,e:=s.activeIssuerLocked(p,"");e!=nil{return Principal{},e};return p,nil}}
 return Principal{},fail(operation.Unauthenticated,"unauthorized")
}
func(s *Service) checkLocked(p Principal)error{c,ok:=s.state.Clients[p.client];if s.unavailable{return fail(operation.Unavailable,"trust_unavailable")};if p.owner!=s||p.generation!=s.state.Revision||!ok||c.PersonID!=p.person||c.DeviceID!=p.device{return fail(operation.Unauthenticated,"unauthorized")};if s.blocked(p.person){return fail(operation.Unavailable,"person_cleanup_pending")};return nil}
func(s *Service) WithCurrentPrincipal(p Principal,consume func(PrincipalSnapshot)error)error{s.mu.RLock();defer s.mu.RUnlock();if err:=s.checkLocked(p);err!=nil{return err};if consume==nil{return fail(operation.Invalid,"validation")};return consume(PrincipalSnapshot{p.client,p.person,p.device,p.generation,true,false})}
func(s *Service) activeIssuerLocked(p Principal,id string)(IssuerSnapshot,error){if err:=s.checkLocked(p);err!=nil{return IssuerSnapshot{},err};for _,r:=range s.state.Issuers{if r.ClientID==p.client&&(id==""||id==r.KeyID){return IssuerSnapshot{p,r.KeyID,r.EnrollmentID,append(ed25519.PublicKey(nil),r.PublicKey...),r.Generation},nil}};return IssuerSnapshot{},fail(operation.Denied,"issuer_unavailable")}
func(s *Service) ActiveIssuer(p Principal)(IssuerSnapshot,error){s.mu.RLock();defer s.mu.RUnlock();return s.activeIssuerLocked(p,"")}
func(s *Service) WithActiveIssuer(p Principal,id string,consume func(IssuerSnapshot)error)error{s.mu.RLock();defer s.mu.RUnlock();r,err:=s.activeIssuerLocked(p,id);if err!=nil{return err};if consume==nil{return fail(operation.Invalid,"validation")};return consume(r)}
func(s *Service) ProducerMetadata()(ProducerMetadata,error){s.mu.RLock();defer s.mu.RUnlock();if s.unavailable||s.producer==nil{return ProducerMetadata{},fail(operation.Unavailable,"producer_unavailable")};return s.metadataLocked(),nil}
func(s *Service) metadataLocked()ProducerMetadata{return ProducerMetadata{1,s.state.InstanceID,s.state.ExecutionOwner,"floe.server:"+s.state.InstanceID,s.producer.KeyID(),base64.RawURLEncoding.EncodeToString(s.producer.PublicKey()),s.producer.Fingerprint()}}
func(s *Service) SignProducerChallenge(data []byte)([]byte,error){s.mu.RLock();defer s.mu.RUnlock();if s.unavailable||s.producer==nil||len(data)>65536{return nil,fail(operation.Unavailable,"producer_unavailable")};return s.producer.SignChallenge(data),nil}
func(s *Service) PreparePairing(person string)(uint64,error){s.mu.RLock();defer s.mu.RUnlock();if s.unavailable{return 0,fail(operation.Unavailable,"trust_unavailable")};if !ValidID(person){return 0,fail(operation.Invalid,"identity_required")};if len(s.state.Cleanup)!=0{return 0,fail(operation.Unavailable,"person_cleanup_pending")};if len(s.state.Clients)>=16||len(s.state.Issuers)+len(s.state.Revoked)>=128{return 0,fail(operation.Limited,"pairing_limit")};for _,c:=range s.state.Clients{if c.PersonID!=person{return 0,fail(operation.Conflict,"person_mismatch")}};return s.state.Revision,nil}
func(s *Service) ActivatePairing(ctx context.Context,a PairingActivation)(PairingCommit,error){
 if err:=ctx.Err();err!=nil{return PairingCommit{},err};if err:=s.WithCurrentOperator(a.Operator,func()error{return nil});err!=nil{return PairingCommit{},err};s.mu.Lock();defer s.mu.Unlock()
 if s.unavailable{return PairingCommit{},fail(operation.Unavailable,"trust_unavailable")};if a.ExpectedRevision!=s.state.Revision||len(s.state.Cleanup)!=0{return PairingCommit{},fail(operation.Conflict,"pairing_conflict")}
 if !ValidID(a.PairingID)||!ValidID(a.PersonID)||!ValidDevice(a.DeviceID)||!ValidID(a.IssuerKeyID)||len(a.IssuerPublicKey)!=ed25519.PublicKeySize||a.Producer!=s.metadataLocked()||a.IssuerFingerprint!=Digest(string(a.IssuerPublicKey))||a.AdminFingerprint!=a.IssuerFingerprint||len(a.TokenHash)!=64||!validDigest(a.PollProofHash){return PairingCommit{},fail(operation.Invalid,"pairing_denied")}
 tokenHash,err:=hex.DecodeString(a.TokenHash);if err!=nil||len(tokenHash)!=32||hex.EncodeToString(tokenHash)!=a.TokenHash{return PairingCommit{},fail(operation.Invalid,"pairing_denied")}
 var challenge struct{Version int `json:"v"`;Operation string `json:"operation"`;ChallengeID string `json:"challenge_id"`;Nonce string `json:"nonce"`;KeyID string `json:"key_id"`;PersonID string `json:"person_id"`;ClientID string `json:"client_id"`;DeviceID string `json:"device_id"`;Audience string `json:"audience"`;Purpose string `json:"purpose"`;Consumer string `json:"consumer"`;Issued int64 `json:"issued_at_unix_ms"`;Expires int64 `json:"expires_at_unix_ms"`}
 if DecodeStrict(a.ChallengeBytes,&challenge,65536,16)!=nil||challenge.Version!=1||challenge.Operation!="enrollment"||challenge.ChallengeID!=a.ChallengeID||challenge.KeyID!=a.IssuerKeyID||challenge.PersonID!=a.PersonID||challenge.ClientID!=a.PairingID||challenge.DeviceID!=a.DeviceID||challenge.Audience!=a.Producer.Audience||challenge.Purpose!="owner_enrollment"||challenge.Consumer!="owner"||challenge.Issued>time.Now().UnixMilli()||challenge.Expires<=time.Now().UnixMilli()||challenge.Expires-challenge.Issued>30000{return PairingCommit{},fail(operation.Denied,"pairing_denied")}
 if _,err=DecodeBase64(challenge.Nonce,32);err!=nil{return PairingCommit{},fail(operation.Denied,"pairing_denied")};if VerifyProof(a.LocalProof,a.ChallengeID,a.IssuerKeyID,a.ChallengeBytes,a.IssuerPublicKey)!=nil{return PairingCommit{},fail(operation.Denied,"pairing_denied")}
 if _,ok:=s.state.Clients[a.PairingID];ok{return PairingCommit{},fail(operation.Conflict,"pairing_conflict")};if _,ok:=s.state.Issuers[a.IssuerKeyID];ok||s.state.Revoked[a.IssuerKeyID]||len(s.state.Clients)>=16||len(s.state.Issuers)+len(s.state.Revoked)>=128{return PairingCommit{},fail(operation.Conflict,"pairing_conflict")};for _,c:=range s.state.Clients{if c.PersonID!=a.PersonID||c.TokenHash==a.TokenHash{return PairingCommit{},fail(operation.Conflict,"pairing_conflict")}}
 next:=clone(s.state);next.Revision++;next.Clients[a.PairingID]=clientRecord{a.PairingID,a.PersonID,a.DeviceID,a.TokenHash,a.PollProofHash};next.Issuers[a.IssuerKeyID]=issuerRecord{a.IssuerKeyID,a.PairingID,a.ChallengeID,append([]byte(nil),a.IssuerPublicKey...),next.Revision};if err:=s.persist(next);err!=nil{return PairingCommit{},err};s.state=next;return PairingCommit{a.PairingID,next.Revision},nil
}
func(s *Service) RevokeClient(ctx context.Context,id string)(RevocationReceipt,error){if err:=ctx.Err();err!=nil{return RevocationReceipt{},err};s.mu.Lock();defer s.mu.Unlock();if s.unavailable{return RevocationReceipt{},fail(operation.Unavailable,"trust_unavailable")};c,ok:=s.state.Clients[id];if !ok{return RevocationReceipt{},fail(operation.Missing,"client_not_found")};next:=clone(s.state);delete(next.Clients,id);for k,r:=range next.Issuers{if r.ClientID==id{delete(next.Issuers,k);next.Revoked[k]=true}};next.Revision++;kind:=PersonAllSources;for _,remaining:=range next.Clients{if remaining.PersonID==c.PersonID{kind=ClientAttempts}};ticket:=CleanupTicket{NewID(),1,c.PersonID,id,kind,next.Revision};next.Cleanup[ticket.ID]=ticket;if err:=s.persist(next);err!=nil{return RevocationReceipt{},err};s.state=next;return RevocationReceipt{id,next.Revision,ticket},nil}
func(s *Service) RevokeIssuer(ctx context.Context,id string)error{if err:=ctx.Err();err!=nil{return err};s.mu.Lock();defer s.mu.Unlock();if s.unavailable{return fail(operation.Unavailable,"trust_unavailable")};if _,ok:=s.state.Issuers[id];!ok{return fail(operation.Missing,"issuer_unavailable")};next:=clone(s.state);delete(next.Issuers,id);next.Revoked[id]=true;next.Revision++;if err:=s.persist(next);err!=nil{return err};s.state=next;return nil}
func(s *Service) PendingCleanup()([]CleanupTicket,error){s.mu.RLock();defer s.mu.RUnlock();if s.unavailable{return nil,fail(operation.Unavailable,"trust_unavailable")};out:=make([]CleanupTicket,0,len(s.state.Cleanup));for _,t:=range s.state.Cleanup{out=append(out,t)};sort.Slice(out,func(i,j int)bool{return out[i].TrustGeneration<out[j].TrustGeneration});return out,nil}
func(s *Service) AcknowledgeCleanup(ctx context.Context,t CleanupTicket,r CleanupReceipt)error{if err:=ctx.Err();err!=nil{return err};s.mu.Lock();defer s.mu.Unlock();if s.unavailable{return fail(operation.Unavailable,"trust_unavailable")};current,ok:=s.state.Cleanup[t.ID];if !ok||current!=t||r.TicketID!=t.ID||r.TicketRevision!=t.Revision||r.TrustGeneration!=t.TrustGeneration||r.IntegrationRevision==0{return fail(operation.Conflict,"cleanup_conflict")};next:=clone(s.state);delete(next.Cleanup,t.ID);next.Revision++;if err:=s.persist(next);err!=nil{return err};s.state=next;return nil}
func(s *Service) Clients()([]ClientSnapshot,error){s.mu.RLock();defer s.mu.RUnlock();if s.unavailable{return nil,fail(operation.Unavailable,"trust_unavailable")};out:=make([]ClientSnapshot,0,len(s.state.Clients));for _,c:=range s.state.Clients{out=append(out,ClientSnapshot{c.ClientID,c.PersonID,c.DeviceID})};return out,nil}
func(s *Service) Issuers()([]IssuerSnapshot,error){s.mu.RLock();defer s.mu.RUnlock();if s.unavailable{return nil,fail(operation.Unavailable,"trust_unavailable")};out:=make([]IssuerSnapshot,0,len(s.state.Issuers));for _,r:=range s.state.Issuers{c:=s.state.Clients[r.ClientID];p:=Principal{c.ClientID,c.PersonID,c.DeviceID,s.state.Revision,s};out=append(out,IssuerSnapshot{p,r.KeyID,r.EnrollmentID,append(ed25519.PublicKey(nil),r.PublicKey...),r.Generation})};return out,nil}
func Result(err error)operation.Result{var e operation.Error;if errors.As(err,&e){return operation.Reject(e.Category,e.Code)};return operation.Reject(operation.Unavailable,"operation_unavailable")}

func validDigest(s string)bool{b,e:=hex.DecodeString(s);return e==nil&&len(b)==32&&hex.EncodeToString(b)==s}
type PairingReadback struct{ClientID,PersonID,DeviceID,IssuerKeyID,IssuerPublicKey,IssuerFingerprint,TokenHash string;Producer ProducerMetadata}
func(s *Service) ReadPairing(ctx context.Context,id,proof string)(PairingReadback,error){if err:=ctx.Err();err!=nil{return PairingReadback{},err};s.mu.RLock();defer s.mu.RUnlock();c,ok:=s.state.Clients[id];if s.unavailable||!ok||s.blocked(c.PersonID)||subtle.ConstantTimeCompare([]byte(c.PollProofHash),[]byte(Digest(proof)))!=1{return PairingReadback{},fail(operation.Conflict,"pairing_repair_required")};for _,r:=range s.state.Issuers{if r.ClientID==id{return PairingReadback{id,c.PersonID,c.DeviceID,r.KeyID,base64.RawURLEncoding.EncodeToString(r.PublicKey),Digest(string(r.PublicKey)),c.TokenHash,s.metadataLocked()},nil}};return PairingReadback{},fail(operation.Conflict,"pairing_repair_required")}

// WithPairingOperation authorizes the operator's hosted setup for one previously bound app operation.
// It supplies a read-fenced identity snapshot, never an app Principal.
func(s *Service) WithPairingOperation(operator OperatorPrincipal,client,person,device string,consume func(PrincipalSnapshot)error)error{if err:=s.WithCurrentOperator(operator,func()error{return nil});err!=nil{return err};s.mu.RLock();defer s.mu.RUnlock();p:=Principal{client,person,device,s.state.Revision,s};if _,err:=s.activeIssuerLocked(p,"");err!=nil{return err};if consume==nil{return fail(operation.Invalid,"validation")};return consume(PrincipalSnapshot{client,person,device,s.state.Revision,true,false})}
