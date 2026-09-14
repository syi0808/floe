package console

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/credentials"
)

func TestAuthorityEnrollmentHTTPFlowAndMethodGuards(t *testing.T) {
	fixture := setup(t)
	clientID, token := fixture.pair()
	privateKey := ed25519.NewKeyFromSeed([]byte("01234567890123456789012345678901"))
	publicKey := privateKey.Public().(ed25519.PublicKey)
	keyID := "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
	beginBody := map[string]any{"key_id": keyID, "public_key": base64.RawURLEncoding.EncodeToString(publicKey), "audience": "local-owner"}
	if unauthorized := fixture.call(http.MethodPost, "/v1/authority/enrollment/begin", beginBody, "invalid-bearer"); unauthorized.Code != http.StatusUnauthorized {
		t.Fatalf("wrong bearer reached enrollment: %d", unauthorized.Code)
	}
	wrongCase := httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8431/v1/authority/enrollment/begin", bytes.NewBufferString(`{"KEY_ID":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","public_key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","audience":"floe.server:00000000-0000-4000-8000-000000000001"}`))
	wrongCase.Host = "127.0.0.1:8431"
	wrongCase.Header.Set("Content-Type", "application/json")
	wrongCase.Header.Set("Authorization", "Bearer "+token)
	wrongCaseResponse := httptest.NewRecorder()
	fixture.console.ServeHTTP(wrongCaseResponse, wrongCase)
	if wrongCaseResponse.Code != http.StatusBadRequest {
		t.Fatalf("case-alias authority field accepted: %d %s", wrongCaseResponse.Code, wrongCaseResponse.Body.String())
	}
	begin := fixture.call(http.MethodPost, "/v1/authority/enrollment/begin", beginBody, token)
	if begin.Code != http.StatusOK {
		t.Fatalf("begin: %d %s", begin.Code, begin.Body.String())
	}
	var beginValue map[string]any
	_ = json.Unmarshal(begin.Body.Bytes(), &beginValue)
	challengeBytes, err := base64.RawURLEncoding.DecodeString(beginValue["challenge_b64url"].(string))
	if err != nil {
		t.Fatal(err)
	}
	proof := authorization.Proof{ChallengeID: beginValue["enrollment_id"].(string), KeyID: keyID, Signature: base64.RawURLEncoding.EncodeToString(ed25519.Sign(privateKey, append([]byte(authorization.SignatureDomain), challengeBytes...)))}
	complete := fixture.call(http.MethodPost, "/v1/authority/enrollment/complete", map[string]any{"enrollment_id": proof.ChallengeID, "challenge_id": proof.ChallengeID, "key_id": proof.KeyID, "signature": proof.Signature}, token)
	if complete.Code != http.StatusOK {
		t.Fatalf("complete: %d %s", complete.Code, complete.Body.String())
	}
	list := fixture.call(http.MethodGet, "/manage/api/authority/enrollments", nil, "")
	if list.Code != http.StatusOK {
		t.Fatalf("list: %d", list.Code)
	}
	var listValue struct {
		Enrollments []map[string]any `json:"enrollments"`
	}
	_ = json.Unmarshal(list.Body.Bytes(), &listValue)
	if len(listValue.Enrollments) != 1 {
		t.Fatalf("enrollments=%d", len(listValue.Enrollments))
	}
	fingerprint := listValue.Enrollments[0]["fingerprint"].(string)
	approveBody, _ := json.Marshal(map[string]any{"enrollment_id": proof.ChallengeID, "fingerprint": fingerprint})
	wrongOrigin := httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8431/manage/api/authority/approve", bytes.NewReader(approveBody))
	wrongOrigin.Host = "127.0.0.1:8431"
	wrongOrigin.Header.Set("Content-Type", "application/json")
	wrongOrigin.Header.Set("Origin", "http://evil.invalid")
	wrongOrigin.Header.Set("X-Floe-CSRF", fixture.csrf)
	wrongOrigin.AddCookie(fixture.cookie)
	wrongOriginResponse := httptest.NewRecorder()
	fixture.console.ServeHTTP(wrongOriginResponse, wrongOrigin)
	if wrongOriginResponse.Code != http.StatusForbidden {
		t.Fatalf("wrong origin reached approval: %d", wrongOriginResponse.Code)
	}
	missingCSRF := httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8431/manage/api/authority/approve", bytes.NewReader(approveBody))
	missingCSRF.Host = "127.0.0.1:8431"
	missingCSRF.Header.Set("Content-Type", "application/json")
	missingCSRF.Header.Set("Origin", "http://127.0.0.1:8431")
	missingCSRF.AddCookie(fixture.cookie)
	missingCSRFResponse := httptest.NewRecorder()
	fixture.console.ServeHTTP(missingCSRFResponse, missingCSRF)
	if missingCSRFResponse.Code != http.StatusUnauthorized {
		t.Fatalf("missing CSRF reached approval: %d", missingCSRFResponse.Code)
	}
	if response := fixture.call(http.MethodGet, "/manage/api/authority/approve", map[string]any{"enrollment_id": proof.ChallengeID, "fingerprint": fingerprint}, ""); response.Code != http.StatusMethodNotAllowed {
		t.Fatalf("GET approve mutated: %d", response.Code)
	}
	approved := fixture.call(http.MethodPost, "/manage/api/authority/approve", map[string]any{"enrollment_id": proof.ChallengeID, "fingerprint": fingerprint}, "")
	if approved.Code != http.StatusOK {
		t.Fatalf("approve: %d %s", approved.Code, approved.Body.String())
	}
	status := fixture.call(http.MethodGet, "/v1/authority/enrollment/"+proof.ChallengeID, nil, token)
	if status.Code != http.StatusOK || !strings.Contains(status.Body.String(), `"active":true`) {
		t.Fatalf("status: %d %s", status.Code, status.Body.String())
	}
	duplicate := httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8431/manage/api/authority/approve", bytes.NewBufferString(`{"enrollment_id":"`+proof.ChallengeID+`","fingerprint":"`+fingerprint+`","nested":{"value":1,"value":2}}`))
	duplicate.Host = "127.0.0.1:8431"
	duplicate.Header.Set("Content-Type", "application/json")
	duplicate.Header.Set("Origin", "http://127.0.0.1:8431")
	duplicate.Header.Set("X-Floe-CSRF", fixture.csrf)
	duplicate.AddCookie(fixture.cookie)
	duplicateResponse := httptest.NewRecorder()
	fixture.console.ServeHTTP(duplicateResponse, duplicate)
	if duplicateResponse.Code != http.StatusBadRequest {
		t.Fatalf("nested duplicate accepted: %d %s", duplicateResponse.Code, duplicateResponse.Body.String())
	}
	revokeGet := fixture.call(http.MethodGet, "/manage/api/authority/revoke", map[string]any{"key_id": keyID}, "")
	if revokeGet.Code != http.StatusMethodNotAllowed {
		t.Fatalf("GET revoke mutated: %d", revokeGet.Code)
	}
	deleted := fixture.call(http.MethodPost, "/manage/api/client/delete", map[string]any{"id": clientID}, "")
	if deleted.Code != http.StatusOK {
		t.Fatalf("client delete: %d %s", deleted.Code, deleted.Body.String())
	}
	if len(fixture.console.authorization.ActiveIssuers()) != 0 {
		t.Fatal("client deletion left issuer active in memory")
	}
}

func TestAuthorityTransportPreservesUnavailableAndFallbackResponses(t *testing.T) {
	fixture := setup(t)
	_, token := fixture.pair()

	unknown := fixture.call(http.MethodGet, "/v1/authority/enrollment-unknown", nil, token)
	if unknown.Code != http.StatusNotFound {
		t.Fatalf("unknown authority route status = %d, want %d", unknown.Code, http.StatusNotFound)
	}

	fixture.console.latchTrustUnavailable()
	unavailable := fixture.call(http.MethodPost, "/v1/authority/enrollment/begin", nil, token)
	if unavailable.Code != http.StatusServiceUnavailable || !strings.Contains(unavailable.Body.String(), `"authority_unavailable"`) {
		t.Fatalf("unavailable enrollment = %d %s", unavailable.Code, unavailable.Body.String())
	}
	adminUnavailable := fixture.call(http.MethodGet, "/manage/api/authority/enrollments", nil, "")
	if adminUnavailable.Code != http.StatusServiceUnavailable || !strings.Contains(adminUnavailable.Body.String(), `"authority_unavailable"`) {
		t.Fatalf("unavailable admin authority = %d %s", adminUnavailable.Code, adminUnavailable.Body.String())
	}
}

func TestAuthorityTrustWriteFailureTombstoneAndCorruptionIsolation(t *testing.T) {
	fixture := setup(t)
	clientID, token := fixture.pair()
	privateKey := ed25519.NewKeyFromSeed([]byte("01234567890123456789012345678901"))
	record := authorization.IssuerRecord{
		KeyID:        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
		EnrollmentID: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
		Principal:    authorization.Principal{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true},
		PublicKey:    privateKey.Public().(ed25519.PublicKey),
	}
	store := consoleTrustStore{console: fixture.console}
	originalDirectory := fixture.console.directory
	fixture.console.directory = filepath.Join(originalDirectory, "missing", "node")
	if err := store.CommitIssuerActivation(record); err == nil {
		t.Fatal("activation reported success after durable write failure")
	}
	fixture.console.directory = originalDirectory
	fixture.console.mu.Lock()
	if len(fixture.console.state.TrustedIssuers) != 0 {
		fixture.console.mu.Unlock()
		t.Fatal("failed activation changed in-memory trust")
	}
	fixture.console.mu.Unlock()
	if err := store.CommitIssuerActivation(record); err != nil {
		t.Fatalf("activation: %v", err)
	}
	if err := store.CommitIssuerRevocation(record); err != nil {
		t.Fatalf("revocation: %v", err)
	}
	if err := store.CommitIssuerActivation(record); err == nil {
		t.Fatal("revoked issuer key resurrected")
	}
	postRenameRecord := record
	postRenameRecord.KeyID = "dddddddd-dddd-4ddd-8ddd-dddddddddddd"
	previousDirectorySync := syncPrivateDirectory
	syncPrivateDirectory = func(string) error { return fmt.Errorf("injected directory sync failure") }
	postRenameError := store.CommitIssuerActivation(postRenameRecord)
	syncPrivateDirectory = previousDirectorySync
	if postRenameError == nil || fixture.console.authorityEngine() != nil {
		t.Fatal("post-rename trust failure left protected authority available")
	}
	reopened, err := New(originalDirectory, fixture.console.address, fixture.vault, nil)
	if err != nil {
		t.Fatalf("reopen tombstone: %v", err)
	}
	if reopened.authorization == nil {
		t.Fatal("valid tombstone disabled unrelated authority state")
	}
	corrupt := cloneState(fixture.console.state)
	corrupt.TrustedIssuers["cccccccc-cccc-4ccc-8ccc-cccccccccccc"] = trustedIssuerRecord{KeyID: "cccccccc-cccc-4ccc-8ccc-cccccccccccc", ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID, PublicKey: []byte{1, 2, 3}}
	if err := fixture.console.save(corrupt); err != nil {
		t.Fatalf("write corrupt trust fixture: %v", err)
	}
	fixture.console.mu.Lock()
	fixture.console.state = corrupt
	fixture.console.mu.Unlock()
	if err := fixture.console.save(cloneState(corrupt)); err != nil {
		t.Fatalf("unrelated state save rewrote trust fixture: %v", err)
	}
	reopened, err = New(originalDirectory, fixture.console.address, fixture.vault, nil)
	if err != nil {
		t.Fatalf("corrupt trust killed chat: %v", err)
	}
	if reopened.authorization != nil {
		t.Fatal("corrupt trust enabled protected authority")
	}
	request := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8431/v1/inference-purposes", nil)
	request.Host = fixture.console.address
	request.Header.Set("Authorization", "Bearer "+token)
	response := httptest.NewRecorder()
	reopened.ServeHTTP(response, request)
	if response.Code != http.StatusOK {
		t.Fatalf("corrupt trust disabled model chat: %d %s", response.Code, response.Body.String())
	}
}

func TestConnectionScopeNoopPreservesEpoch(t *testing.T) {
	fixture := setup(t)
	_, token := fixture.pair()
	connectionID := fixtureConnectionID("github.issues")
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: "github.issues", PersonID: fixturePersonID, Scope: map[string]any{"owner": "floe", "repository": "server"}, Incarnation: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", Epoch: 1}
	before := fixture.console.state.Connections[connectionID]
	fixture.console.mu.Unlock()
	unchanged := fixture.call(http.MethodPatch, "/v1/connectors/github.issues/scope", map[string]any{"schema_version": 1, "connection_id": connectionID, "connection_revision": before.Revision, "scope": map[string]any{"owner": "floe", "repository": "server"}}, token)
	if unchanged.Code != http.StatusOK {
		t.Fatalf("no-op scope update: %d %s", unchanged.Code, unchanged.Body.String())
	}
	fixture.console.mu.Lock()
	afterNoop := fixture.console.state.Connections[connectionID]
	fixture.console.mu.Unlock()
	if afterNoop.Epoch != before.Epoch || afterNoop.Revision != before.Revision {
		t.Fatalf("no-op changed source identity: before=%+v after=%+v", before, afterNoop)
	}
	changed := fixture.call(http.MethodPatch, "/v1/connectors/github.issues/scope", map[string]any{"schema_version": 1, "connection_id": connectionID, "connection_revision": before.Revision, "scope": map[string]any{"owner": "floe", "repository": "changed"}}, token)
	if changed.Code != http.StatusOK {
		t.Fatalf("scope update: %d %s", changed.Code, changed.Body.String())
	}
	fixture.console.mu.Lock()
	afterChange := fixture.console.state.Connections[connectionID]
	fixture.console.mu.Unlock()
	if afterChange.Epoch != before.Epoch+1 || afterChange.Incarnation != before.Incarnation {
		t.Fatalf("scope change did not fence source: before=%+v after=%+v", before, afterChange)
	}
}

func TestTrustCollectionsMissingDuplicateAndOverCapStayQuarantined(t *testing.T) {
	fixture := setup(t)
	_, token := fixture.pair()
	statePath := filepath.Join(fixture.console.directory, "state.json")
	data, err := os.ReadFile(statePath)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		t.Fatal(err)
	}
	delete(fields, "trusted_issuers")
	delete(fields, "revoked_issuer_keys")
	data, _ = json.Marshal(fields)
	if err := os.WriteFile(statePath, data, 0600); err != nil {
		t.Fatal(err)
	}
	reopened, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil || reopened.authorization != nil {
		t.Fatalf("missing trust collections were trusted: console=%v auth=%v", err, reopened.authorization)
	}
	if err := reopened.save(cloneState(reopened.state)); err != nil {
		t.Fatal(err)
	}
	reopened, err = New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil || reopened.authorization != nil {
		t.Fatalf("missing trust quarantine was lost after save: console=%v auth=%v", err, reopened.authorization)
	}
	data, _ = os.ReadFile(statePath)
	_ = json.Unmarshal(data, &fields)
	duplicate := json.RawMessage(`{"same":{},"same":{}}`)
	fields["trusted_issuers"] = duplicate
	fields["revoked_issuer_keys"] = json.RawMessage(`{}`)
	delete(fields, "trust_corrupt")
	delete(fields, "trust_quarantine")
	data, _ = json.Marshal(fields)
	if err := os.WriteFile(statePath, data, 0600); err != nil {
		t.Fatal(err)
	}
	reopened, err = New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil || reopened.authorization != nil {
		t.Fatalf("duplicate trust keys were trusted: console=%v auth=%v", err, reopened.authorization)
	}
	issuers := make(map[string]json.RawMessage, maxRetainedIssuerIdentities+1)
	for index := 0; index <= maxRetainedIssuerIdentities; index++ {
		issuers[fmt.Sprintf("key-%d", index)] = json.RawMessage(`{}`)
	}
	fields["trusted_issuers"], _ = json.Marshal(issuers)
	fields["revoked_issuer_keys"] = json.RawMessage(`{}`)
	delete(fields, "trust_corrupt")
	delete(fields, "trust_quarantine")
	data, _ = json.Marshal(fields)
	if err := os.WriteFile(statePath, data, 0600); err != nil {
		t.Fatal(err)
	}
	reopened, err = New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil || reopened.authorization != nil {
		t.Fatalf("over-cap trust killed chat or was trusted: console=%v auth=%v", err, reopened.authorization)
	}
	request := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8431/v1/inference-purposes", nil)
	request.Host = fixture.console.address
	request.Header.Set("Authorization", "Bearer "+token)
	response := httptest.NewRecorder()
	reopened.ServeHTTP(response, request)
	if response.Code != http.StatusOK {
		t.Fatalf("trust quarantine disabled model chat: %d %s", response.Code, response.Body.String())
	}
}

type blockingActivationStore struct {
	delegate consoleTrustStore
	entered  chan struct{}
	release  chan struct{}
	once     sync.Once
}

func (store *blockingActivationStore) LoadIssuers() ([]authorization.IssuerRecord, error) {
	return store.delegate.LoadIssuers()
}

func (store *blockingActivationStore) IsIssuerKeyRevoked(keyID string) (bool, error) {
	return store.delegate.IsIssuerKeyRevoked(keyID)
}

func (store *blockingActivationStore) CommitIssuerActivation(record authorization.IssuerRecord) error {
	store.once.Do(func() { close(store.entered) })
	<-store.release
	return store.delegate.CommitIssuerActivation(record)
}

func (store *blockingActivationStore) CommitIssuerRevocation(record authorization.IssuerRecord) error {
	return store.delegate.CommitIssuerRevocation(record)
}

func TestClientDeleteFencesBlockedIssuerActivation(t *testing.T) {
	fixture := setup(t)
	clientID, _ := fixture.pair()
	store := &blockingActivationStore{delegate: consoleTrustStore{console: fixture.console}, entered: make(chan struct{}), release: make(chan struct{})}
	engine, err := authorization.New(authorization.Options{Store: store})
	if err != nil {
		t.Fatal(err)
	}
	fixture.console.authorization = engine
	privateKey := ed25519.NewKeyFromSeed([]byte("01234567890123456789012345678901"))
	principal := authorization.Principal{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true}
	enrollment, challenge, err := engine.BeginEnrollment(principal, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", privateKey.Public().(ed25519.PublicKey), "local-owner")
	if err != nil {
		t.Fatal(err)
	}
	proof := authorization.Proof{ChallengeID: challenge.ID, KeyID: enrollment.KeyID, Signature: base64.RawURLEncoding.EncodeToString(ed25519.Sign(privateKey, append([]byte(authorization.SignatureDomain), challenge.Bytes...)))}
	if err := engine.CompleteEnrollment(enrollment.ID, principal, proof); err != nil {
		t.Fatal(err)
	}
	activationResult := make(chan error, 1)
	go func() { activationResult <- engine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, true) }()
	select {
	case <-store.entered:
	case <-time.After(time.Second):
		t.Fatal("activation did not reach durable store")
	}
	deletionResult := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		deletionResult <- fixture.call(http.MethodPost, "/manage/api/client/delete", map[string]any{"id": clientID}, "")
	}()
	deleted := false
	for attempt := 0; attempt < 100; attempt++ {
		fixture.console.mu.Lock()
		_, exists := fixture.console.state.Clients[clientID]
		fixture.console.mu.Unlock()
		if !exists {
			deleted = true
			break
		}
		time.Sleep(time.Millisecond)
	}
	if !deleted {
		t.Fatal("client deletion did not durably remove pairing while activation was blocked")
	}
	close(store.release)
	select {
	case response := <-deletionResult:
		if response.Code != http.StatusOK {
			t.Fatalf("client delete: %d %s", response.Code, response.Body.String())
		}
	case <-time.After(time.Second):
		t.Fatal("client deletion remained blocked after activation release")
	}
	if err := <-activationResult; err == nil {
		t.Fatal("activation succeeded after pairing deletion")
	}
	if len(engine.ActiveIssuers()) != 0 {
		t.Fatal("blocked activation left issuer active")
	}
	reopened, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil {
		t.Fatal(err)
	}
	if reopened.authorization == nil {
		t.Fatal("blocked activation/deletion left restart trust unavailable")
	}
}

func TestIndeterminateStateSaveLatchesCachedSourceAuthority(t *testing.T) {
	fixture := setup(t)
	clientID, _ := fixture.pair()
	connectionID := fixtureConnectionID("gmail")
	fixture.console.mu.Lock()
	executionOwner := fixture.console.state.ExecutionOwnerID
	credentialName, _ := credentials.ConnectionName("FLOE_GMAIL_OAUTH", connectionID, fixturePersonID)
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: "gmail", PersonID: fixturePersonID, Scope: map[string]any{}, Credential: credentialName, Incarnation: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", Epoch: 1, ProviderIdentity: "provider-subject"}
	fixture.console.mu.Unlock()
	privateKey := ed25519.NewKeyFromSeed([]byte("01234567890123456789012345678901"))
	principal := authorization.Principal{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true}
	record := authorization.IssuerRecord{KeyID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", EnrollmentID: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", Principal: principal, PublicKey: privateKey.Public().(ed25519.PublicKey)}
	store := consoleTrustStore{console: fixture.console}
	if err := store.CommitIssuerActivation(record); err != nil {
		t.Fatal(err)
	}
	cachedEngine, err := authorization.New(authorization.Options{Store: store})
	if err != nil {
		t.Fatal(err)
	}
	queryDigest := sha256.Sum256([]byte("query"))
	request := authorization.Request{Audience: "local-owner", Purpose: "quick_response", Consumer: "owner", Policy: authorization.PolicyReference{Incarnation: "cccccccc-cccc-4ccc-8ccc-cccccccccccc", Epoch: 1}, Source: authorization.SourceReference{ConnectorID: "gmail", ConnectionID: connectionID, ExecutionOwner: executionOwner, Incarnation: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", Epoch: 1}, Grant: authorization.GrantReference{ID: "dddddddd-dddd-4ddd-8ddd-dddddddddddd", Incarnation: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee", Epoch: 1}, Resources: []string{"source"}, QueryDigest: queryDigest, MaxItems: 1, MaxBytes: 1}
	if _, err := cachedEngine.IssueAdmission(principal, request, fixture.console); err != nil {
		t.Fatalf("cached authority was not initially usable: %v", err)
	}
	previousDirectorySync := syncPrivateDirectory
	syncPrivateDirectory = func(string) error { return fmt.Errorf("injected directory sync failure") }
	deleteResponse := fixture.call(http.MethodPost, "/manage/api/client/delete", map[string]any{"id": clientID}, "")
	syncPrivateDirectory = previousDirectorySync
	if deleteResponse.Code != http.StatusInternalServerError {
		t.Fatalf("indeterminate delete status: %d %s", deleteResponse.Code, deleteResponse.Body.String())
	}
	if fixture.console.authorityEngine() != nil {
		t.Fatal("indeterminate delete left authority available to new requests")
	}
	if _, err := cachedEngine.IssueAdmission(principal, request, fixture.console); err == nil {
		t.Fatal("cached authority bypassed indeterminate trust latch")
	}
	reopened, err := New(fixture.console.directory, fixture.console.address, fixture.vault, nil)
	if err != nil {
		t.Fatal(err)
	}
	if reopened.authorization == nil {
		t.Fatal("reopen trust state was not self-consistent after client delete")
	}
}
