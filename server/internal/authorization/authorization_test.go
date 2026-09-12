package authorization

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"
)

type testClock struct {
	now     time.Time
	elapsed time.Duration
}

func (clock *testClock) Now() time.Time           { return clock.now }
func (clock *testClock) Monotonic() time.Duration { return clock.elapsed }
func (clock *testClock) advance(d time.Duration)  { clock.elapsed += d; clock.now = clock.now.Add(d) }

type testStore struct {
	records           []IssuerRecord
	activateErr       error
	revokeErr         error
	activations       int
	revocations       int
	revokedKeys       map[string]bool
	activationStarted chan struct{}
	activationRelease chan struct{}
}

func (store *testStore) LoadIssuers() ([]IssuerRecord, error) {
	return append([]IssuerRecord(nil), store.records...), nil
}
func (store *testStore) IsIssuerKeyRevoked(keyID string) (bool, error) {
	return store.revokedKeys[keyID], nil
}
func (store *testStore) CommitIssuerActivation(r IssuerRecord) error {
	store.activations++
	if store.activationStarted != nil {
		close(store.activationStarted)
		<-store.activationRelease
	}
	if store.activateErr != nil {
		return store.activateErr
	}
	store.records = append(store.records, r)
	return nil
}
func (store *testStore) CommitIssuerRevocation(r IssuerRecord) error {
	store.revocations++
	if store.revokeErr != nil {
		return store.revokeErr
	}
	if store.revokedKeys == nil {
		store.revokedKeys = make(map[string]bool)
	}
	store.revokedKeys[r.KeyID] = true
	for index := range store.records {
		if store.records[index].KeyID == r.KeyID {
			store.records = append(store.records[:index], store.records[index+1:]...)
			break
		}
	}
	return nil
}

type testAuthority struct {
	active bool
	calls  int
}

type blockingAuthority struct {
	entered    chan struct{}
	continueCh chan struct{}
	active     bool
}

type failingAuthority struct{ err error }

func (authority *failingAuthority) WithCurrentSource(Principal, SourceReference, func(SourceSnapshot) error) error {
	return authority.err
}

func (authority *blockingAuthority) WithCurrentSource(p Principal, ref SourceReference, consume func(SourceSnapshot) error) error {
	select {
	case authority.entered <- struct{}{}:
	default:
	}
	<-authority.continueCh
	return consume(SourceSnapshot{SourceReference: ref, PersonID: p.PersonID, Active: authority.active})
}

func (authority *testAuthority) WithCurrentSource(p Principal, ref SourceReference, consume func(SourceSnapshot) error) error {
	authority.calls++
	return consume(SourceSnapshot{SourceReference: ref, PersonID: p.PersonID, Active: authority.active})
}

func testPrincipal() Principal {
	return Principal{ClientID: "client-1", PersonID: "11111111-1111-4111-8111-111111111111", DeviceID: "device-1", Authenticated: true}
}
func testKeyID() string { return "22222222-2222-4222-8222-222222222222" }
func testRequest() Request {
	queryDigest := sha256.Sum256([]byte(`{"messages":[{"role":"user","content":"hi"}]}`))
	return Request{Audience: "local-producer", Purpose: "everyday_assistance", Consumer: "day-canvas", Policy: PolicyReference{"33333333-3333-4333-8333-333333333333", 2}, Source: SourceReference{"gmail", "44444444-4444-4444-8444-444444444444", "person-runtime", "55555555-5555-4555-8555-555555555555", 7}, Grant: GrantReference{"66666666-6666-4666-8666-666666666666", "77777777-7777-4777-8777-777777777777", 3}, Resources: []string{"mail.read"}, QueryDigest: queryDigest, MaxItems: 10, MaxBytes: 4096}
}
func testEngine(t *testing.T) (*Engine, ed25519.PrivateKey, Principal, *testClock, *testAuthority, *testStore) {
	t.Helper()
	clock := &testClock{now: time.Unix(1_700_000_000, 0)}
	store := &testStore{}
	engine, err := New(Options{Clock: clock, Store: store})
	if err != nil {
		t.Fatal(err)
	}
	principal := testPrincipal()
	private := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{1}, ed25519.SeedSize))
	enrollment, challenge, err := engine.BeginEnrollment(principal, testKeyID(), private.Public().(ed25519.PublicKey), "local-owner")
	if err != nil {
		t.Fatal(err)
	}
	proof := signProof(challenge, testKeyID(), private)
	if err := engine.CompleteEnrollment(enrollment.ID, principal, proof); err != nil {
		t.Fatal(err)
	}
	if err := engine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, true); err != nil {
		t.Fatal(err)
	}
	authority := &testAuthority{active: true}
	return engine, private, principal, clock, authority, store
}
func signProof(ch Challenge, keyID string, private ed25519.PrivateKey) Proof {
	sig := ed25519.Sign(private, append([]byte(SignatureDomain), ch.Bytes...))
	return Proof{ChallengeID: ch.ID, KeyID: keyID, Signature: base64.RawURLEncoding.EncodeToString(sig)}
}

func TestEnrollmentRequiresPoPAndAdminAndDurableCommit(t *testing.T) {
	engine, private, principal, _, _, store := testEngine(t)
	if store.activations != 1 {
		t.Fatalf("activations=%d", store.activations)
	}
	_ = engine
	_ = private
	_ = principal
	failedStore := &testStore{activateErr: errors.New("disk full")}
	clock := &testClock{now: time.Unix(1_700_000_000, 0)}
	e, err := New(Options{Clock: clock, Store: failedStore})
	if err != nil {
		t.Fatal(err)
	}
	pub, key, err := ed25519.GenerateKey(nil)
	if err != nil {
		t.Fatal(err)
	}
	enrollment, ch, err := e.BeginEnrollment(testPrincipal(), "88888888-8888-4888-8888-888888888888", pub, "local-owner")
	if err != nil {
		t.Fatal(err)
	}
	if err := e.CompleteEnrollment(enrollment.ID, testPrincipal(), signProof(ch, enrollment.KeyID, key)); err != nil {
		t.Fatal(err)
	}
	if err := e.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, true); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("want durable failure, got %v", err)
	}
	if _, err := e.IssueAdmission(testPrincipal(), testRequest(), &testAuthority{active: true}); !errors.Is(err, ErrDenied) {
		t.Fatalf("failed activation trusted unexpectedly: %v", err)
	}
}

func TestAdmissionReleaseExactSignatureSourceAndReplay(t *testing.T) {
	engine, private, principal, _, authority, _ := testEngine(t)
	request := testRequest()
	challenge, err := engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	proof := signProof(challenge, testKeyID(), private)
	id, got, err := engine.ClaimAdmission(principal, proof, authority)
	if err != nil {
		t.Fatal(err)
	}
	if !requestsEqual(request, got) {
		t.Fatal("request changed")
	}
	if _, _, err := engine.ClaimAdmission(principal, proof, authority); !errors.Is(err, ErrReplay) {
		t.Fatalf("replay accepted: %v", err)
	}
	release, err := engine.StageResult(id, principal, request, []byte("private result"), 1)
	if err != nil {
		t.Fatal(err)
	}
	out, err := engine.ClaimRelease(principal, signProof(Challenge{ID: release.ID, Bytes: release.Bytes}, testKeyID(), private), authority)
	if err != nil {
		t.Fatal(err)
	}
	if string(out) != "private result" {
		t.Fatalf("output=%q", out)
	}
	if _, err := engine.ClaimRelease(principal, signProof(Challenge{ID: release.ID, Bytes: release.Bytes}, testKeyID(), private), authority); !errors.Is(err, ErrReplay) {
		t.Fatalf("release replay: %v", err)
	}
	challenge, err = engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	authority.active = false
	if _, _, err := engine.ClaimAdmission(principal, signProof(challenge, testKeyID(), private), authority); !errors.Is(err, ErrDenied) {
		t.Fatalf("stale source accepted: %v", err)
	}
}

func TestReleaseCancelDuringAuthorityCheckDenies(t *testing.T) {
	engine, private, principal, _, _, _ := testEngine(t)
	request := testRequest()
	authority := &testAuthority{active: true}
	admission, err := engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	id, _, err := engine.ClaimAdmission(principal, signProof(admission, testKeyID(), private), authority)
	if err != nil {
		t.Fatal(err)
	}
	release, err := engine.StageResult(id, principal, request, []byte("secret"), 1)
	if err != nil {
		t.Fatal(err)
	}
	blocking := &blockingAuthority{entered: make(chan struct{}, 1), continueCh: make(chan struct{}), active: true}
	done := make(chan error, 1)
	go func() {
		_, err := engine.ClaimRelease(principal, signProof(Challenge{ID: release.ID, Bytes: release.Bytes}, testKeyID(), private), blocking)
		done <- err
	}()
	<-blocking.entered
	engine.CancelRelease(release.ID)
	close(blocking.continueCh)
	if err := <-done; !errors.Is(err, ErrReplay) {
		t.Fatalf("cancelled release returned %v", err)
	}
}

func TestConcurrentReleaseHasOneWinner(t *testing.T) {
	engine, private, principal, _, authority, _ := testEngine(t)
	request := testRequest()
	admission, err := engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	id, _, err := engine.ClaimAdmission(principal, signProof(admission, testKeyID(), private), authority)
	if err != nil {
		t.Fatal(err)
	}
	release, err := engine.StageResult(id, principal, request, []byte("secret"), 1)
	if err != nil {
		t.Fatal(err)
	}
	blocking := &blockingAuthority{entered: make(chan struct{}, 1), continueCh: make(chan struct{}), active: true}
	first := make(chan error, 1)
	second := make(chan error, 1)
	proof := signProof(Challenge{ID: release.ID, Bytes: release.Bytes}, testKeyID(), private)
	go func() { _, claimErr := engine.ClaimRelease(principal, proof, blocking); first <- claimErr }()
	<-blocking.entered
	go func() { _, claimErr := engine.ClaimRelease(principal, proof, blocking); second <- claimErr }()
	if claimErr := <-second; !errors.Is(claimErr, ErrReplay) {
		t.Fatalf("second release result: %v", claimErr)
	}
	close(blocking.continueCh)
	if claimErr := <-first; claimErr != nil {
		t.Fatalf("first release result: %v", claimErr)
	}
}

func TestAuthorityFailureCleansCheckingState(t *testing.T) {
	engine, private, principal, _, authority, _ := testEngine(t)
	request := testRequest()
	challenge, err := engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	failing := &failingAuthority{err: errors.New("owner callback failed")}
	if _, _, err := engine.ClaimAdmission(principal, signProof(challenge, testKeyID(), private), failing); err == nil {
		t.Fatal("callback failure accepted")
	}
	if _, _, err := engine.ClaimAdmission(principal, signProof(challenge, testKeyID(), private), authority); !errors.Is(err, ErrReplay) {
		t.Fatalf("failed challenge remained usable: %v", err)
	}
	newChallenge, err := engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	admissionID, _, err := engine.ClaimAdmission(principal, signProof(newChallenge, testKeyID(), private), authority)
	if err != nil {
		t.Fatal(err)
	}
	release, err := engine.StageResult(admissionID, principal, request, []byte("secret"), 1)
	if err != nil {
		t.Fatal(err)
	}
	releaseProof := signProof(Challenge{ID: release.ID, Bytes: release.Bytes}, testKeyID(), private)
	if _, err := engine.ClaimRelease(principal, releaseProof, failing); err == nil {
		t.Fatal("release callback failure accepted")
	}
	if _, err := engine.ClaimRelease(principal, releaseProof, authority); !errors.Is(err, ErrReplay) {
		t.Fatalf("failed release remained usable: %v", err)
	}
}

func TestEnrollmentQuotaSurvivesLocalConfirmation(t *testing.T) {
	clock := &testClock{now: time.Unix(1_700_000_000, 0)}
	store := &testStore{}
	engine, err := New(Options{Clock: clock, Store: store})
	if err != nil {
		t.Fatal(err)
	}
	principal := testPrincipal()
	for index := 0; index < MaxPendingPerClient; index++ {
		privateKey := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{byte(index + 1)}, ed25519.SeedSize))
		keyID := fmt.Sprintf("aaaaaaaa-aaaa-4aaa-8aaa-%012x", index+1)
		enrollment, challenge, beginErr := engine.BeginEnrollment(principal, keyID, privateKey.Public().(ed25519.PublicKey), "local-owner")
		if beginErr != nil {
			t.Fatalf("begin %d: %v", index, beginErr)
		}
		if completeErr := engine.CompleteEnrollment(enrollment.ID, principal, signProof(challenge, keyID, privateKey)); completeErr != nil {
			t.Fatalf("complete %d: %v", index, completeErr)
		}
	}
	if _, _, err := engine.BeginEnrollment(principal, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", ed25519.NewKeyFromSeed(bytes.Repeat([]byte{9}, ed25519.SeedSize)).Public().(ed25519.PublicKey), "local-owner"); !errors.Is(err, ErrDenied) {
		t.Fatalf("confirmed enrollment quota ignored: %v", err)
	}
}

func TestAdmittedActivationFinishesAfterTTL(t *testing.T) {
	clock := &testClock{now: time.Unix(1_700_000_000, 0)}
	store := &testStore{activationStarted: make(chan struct{}), activationRelease: make(chan struct{})}
	engine, err := New(Options{Clock: clock, Store: store})
	if err != nil {
		t.Fatal(err)
	}
	privateKey := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{1}, ed25519.SeedSize))
	principal := testPrincipal()
	enrollment, challenge, err := engine.BeginEnrollment(principal, testKeyID(), privateKey.Public().(ed25519.PublicKey), "local-owner")
	if err != nil {
		t.Fatal(err)
	}
	if err := engine.CompleteEnrollment(enrollment.ID, principal, signProof(challenge, testKeyID(), privateKey)); err != nil {
		t.Fatal(err)
	}
	activationResult := make(chan error, 1)
	go func() { activationResult <- engine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, true) }()
	<-store.activationStarted
	clock.advance(ChallengeTTL + time.Millisecond)
	close(store.activationRelease)
	if activationErr := <-activationResult; activationErr != nil {
		t.Fatalf("admitted activation result: %v", activationErr)
	}
	if store.revocations != 0 {
		t.Fatalf("unexpected rollback revocations=%d", store.revocations)
	}
	if _, err := engine.IssueAdmission(principal, testRequest(), &testAuthority{active: true}); err != nil {
		t.Fatalf("runtime trust disagrees with durable activation: %v", err)
	}
}

func TestRejectionLosesToAdmittedActivation(t *testing.T) {
	clock := &testClock{now: time.Unix(1_700_000_000, 0)}
	store := &testStore{activationStarted: make(chan struct{}), activationRelease: make(chan struct{})}
	engine, err := New(Options{Clock: clock, Store: store})
	if err != nil {
		t.Fatal(err)
	}
	privateKey := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{1}, ed25519.SeedSize))
	principal := testPrincipal()
	enrollment, challenge, err := engine.BeginEnrollment(principal, testKeyID(), privateKey.Public().(ed25519.PublicKey), "local-owner")
	if err != nil {
		t.Fatal(err)
	}
	if err := engine.CompleteEnrollment(enrollment.ID, principal, signProof(challenge, testKeyID(), privateKey)); err != nil {
		t.Fatal(err)
	}
	activationResult := make(chan error, 1)
	go func() { activationResult <- engine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, true) }()
	<-store.activationStarted
	rejectionResult := make(chan error, 1)
	go func() { rejectionResult <- engine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, false) }()
	select {
	case rejectionErr := <-rejectionResult:
		t.Fatalf("rejection completed before activation: %v", rejectionErr)
	case <-time.After(10 * time.Millisecond):
	}
	close(store.activationRelease)
	if activationErr := <-activationResult; activationErr != nil {
		t.Fatal(activationErr)
	}
	if rejectionErr := <-rejectionResult; !errors.Is(rejectionErr, ErrConflict) {
		t.Fatalf("rejection result: %v", rejectionErr)
	}
	if _, err := engine.IssueAdmission(principal, testRequest(), &testAuthority{active: true}); err != nil {
		t.Fatalf("admitted issuer unavailable: %v", err)
	}
}

func TestReturnedBytesAndRequestsAreCopies(t *testing.T) {
	engine, private, principal, _, authority, _ := testEngine(t)
	request := testRequest()
	challenge, err := engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	originalBytes := append([]byte(nil), challenge.Bytes...)
	challenge.Bytes[0] ^= 1
	request.Resources[0] = "calendar.read"
	_, returned, err := engine.ClaimAdmission(principal, signProof(Challenge{ID: challenge.ID, Bytes: originalBytes}, testKeyID(), private), authority)
	if err != nil {
		t.Fatal(err)
	}
	if returned.Resources[0] != "mail.read" {
		t.Fatalf("stored request aliased caller: %v", returned.Resources)
	}
}

func TestStrictBytesAndModifiedFields(t *testing.T) {
	engine, private, principal, _, authority, _ := testEngine(t)
	challenge, err := engine.IssueAdmission(principal, testRequest(), authority)
	if err != nil {
		t.Fatal(err)
	}
	if err := ParseChallengeBytes(challenge.Bytes); err != nil {
		t.Fatal(err)
	}
	modified := append([]byte(nil), challenge.Bytes...)
	modified[len(modified)-1] ^= 1
	if err := ParseChallengeBytes(modified); err != nil { /* mutation may remain valid JSON; signature still must fail */
	}
	proof := signProof(Challenge{ID: challenge.ID, Bytes: modified}, testKeyID(), private)
	if _, _, err := engine.ClaimAdmission(principal, proof, authority); !errors.Is(err, ErrDenied) {
		t.Fatalf("modified bytes accepted: %v", err)
	}
	var object map[string]any
	if err := json.Unmarshal(challenge.Bytes, &object); err != nil {
		t.Fatal(err)
	}
	duplicate := string(challenge.Bytes[:len(challenge.Bytes)-1]) + `,"operation":"release"}`
	if err := ParseChallengeBytes([]byte(duplicate)); !errors.Is(err, ErrInvalid) {
		t.Fatalf("duplicate key accepted: %v", err)
	}
}

func TestExpiryAndStageCaps(t *testing.T) {
	engine, private, principal, clock, authority, _ := testEngine(t)
	request := testRequest()
	challenge, err := engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	clock.advance(ChallengeTTL + time.Millisecond)
	if _, _, err := engine.ClaimAdmission(principal, signProof(challenge, testKeyID(), private), authority); !errors.Is(err, ErrExpired) {
		t.Fatalf("late admission: %v", err)
	}
	clock.advance(-ChallengeTTL - time.Millisecond)
	authority.active = true
	challenge, err = engine.IssueAdmission(principal, request, authority)
	if err != nil {
		t.Fatal(err)
	}
	id, _, err := engine.ClaimAdmission(principal, signProof(challenge, testKeyID(), private), authority)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := engine.StageResult(id, principal, request, make([]byte, MaxStageBytesPerResult+1), 1); !errors.Is(err, ErrDenied) {
		t.Fatalf("oversize stage: %v", err)
	}
}

func TestRevokeDurabilityAndOrdering(t *testing.T) {
	engine, _, principal, _, authority, store := testEngine(t)
	store.revokeErr = errors.New("disk full")
	if err := engine.RevokeIssuer(testKeyID()); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("want revoke failure: %v", err)
	}
	if _, err := engine.IssueAdmission(principal, testRequest(), authority); err != nil {
		t.Fatalf("failed revoke removed issuer: %v", err)
	}
	store.revokeErr = nil
	if err := engine.RevokeIssuer(testKeyID()); err != nil {
		t.Fatal(err)
	}
	if _, err := engine.IssueAdmission(principal, testRequest(), authority); !errors.Is(err, ErrDenied) {
		t.Fatalf("revoked issuer accepted: %v", err)
	}
	private := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{1}, ed25519.SeedSize))
	if _, _, err := engine.BeginEnrollment(principal, testKeyID(), private.Public().(ed25519.PublicKey), "local-owner"); !errors.Is(err, ErrDenied) {
		t.Fatalf("revoked key re-enrolled: %v", err)
	}
}

func TestSharedPositiveVector(t *testing.T) {
	path := filepath.Join("..", "..", "..", "fixtures", "remote-authorization", "v1.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Positive struct {
			ChallengeBytes string `json:"challenge_bytes"`
			Signature      string `json:"signature_b64url"`
		} `json:"positive"`
		PublicKey string `json:"public_key_hex"`
		Negative  []struct {
			Name            string `json:"name"`
			Signature       string `json:"signature_b64url"`
			ChallengeSuffix string `json:"challenge_suffix"`
			Mutation        string `json:"mutation"`
			Replacement     string `json:"replacement"`
		} `json:"negative"`
	}
	if err := json.Unmarshal(data, &fixture); err != nil {
		t.Fatal(err)
	}
	if err := ParseChallengeBytes([]byte(fixture.Positive.ChallengeBytes)); err != nil {
		t.Fatal(err)
	}
	publicKey := mustHex(t, fixture.PublicKey)
	signature, err := base64.RawURLEncoding.DecodeString(fixture.Positive.Signature)
	if err != nil {
		t.Fatal(err)
	}
	if !ed25519.Verify(ed25519.PublicKey(publicKey), append([]byte(SignatureDomain), []byte(fixture.Positive.ChallengeBytes)...), signature) {
		t.Fatal("fixture signature does not verify")
	}
	for _, negative := range fixture.Negative {
		switch negative.Name {
		case "signature_modified":
			modified, decodeErr := base64.RawURLEncoding.DecodeString(negative.Signature)
			if decodeErr != nil || ed25519.Verify(ed25519.PublicKey(publicKey), append([]byte(SignatureDomain), []byte(fixture.Positive.ChallengeBytes)...), modified) {
				t.Fatalf("%s accepted", negative.Name)
			}
		case "padded_base64":
			if _, decodeErr := decodeB64(negative.Signature, ed25519.SignatureSize); decodeErr == nil {
				t.Fatalf("%s accepted", negative.Name)
			}
		case "changed_source", "changed_grant", "changed_policy", "changed_query", "changed_audience", "changed_operation":
			mutated := bytes.Replace([]byte(fixture.Positive.ChallengeBytes), []byte(negative.Mutation), []byte(negative.Replacement), 1)
			if err := ParseChallengeBytes(mutated); err != nil && negative.Name != "changed_operation" {
				t.Fatalf("%s structural rejection: %v", negative.Name, err)
			}
			if ed25519.Verify(ed25519.PublicKey(publicKey), append([]byte(SignatureDomain), mutated...), signature) {
				t.Fatalf("%s signature accepted", negative.Name)
			}
		default:
			base := fixture.Positive.ChallengeBytes[:len(fixture.Positive.ChallengeBytes)-1]
			if err := ParseChallengeBytes([]byte(base + negative.ChallengeSuffix)); err == nil {
				t.Fatalf("%s accepted", negative.Name)
			}
		}
	}
}

func mustHex(t *testing.T, value string) []byte {
	t.Helper()
	out, err := hex.DecodeString(value)
	if err != nil {
		t.Fatal(err)
	}
	return out
}
