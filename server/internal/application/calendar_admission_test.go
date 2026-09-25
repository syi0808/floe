package application

import (
	"context"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"strings"
	"testing"
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/connections"
)

type calendarAuthorityIdentityRuntime struct {
	identity string
}

func (runtime *calendarAuthorityIdentityRuntime) Action(context.Context, string) (any, error) {
	return map[string]any{"status": "connected"}, nil
}

func (*calendarAuthorityIdentityRuntime) BindCredential(string) error           { return nil }
func (*calendarAuthorityIdentityRuntime) Ready() bool                           { return true }
func (*calendarAuthorityIdentityRuntime) Token(context.Context) (string, error) { return "token", nil }
func (runtime *calendarAuthorityIdentityRuntime) ProviderIdentity(context.Context) (string, error) {
	return runtime.identity, nil
}
func (runtime *calendarAuthorityIdentityRuntime) WithVerifiedProviderIdentity(expectedCredential, expected string, consume func() error) error {
	if expectedCredential == "" || expected != runtime.identity {
		return authorization.ErrDenied
	}
	return consume()
}

func TestCalendarSourcePreviewIsProducerSignedAndUsesVerifiedIdentity(t *testing.T) {
	fixture := setup(t)
	clientID, token := fixture.pair()
	fixture.ownConnector("calendar.google")
	connectionID := fixtureConnectionID("calendar.google")
	fixture.console.calendarAuth = &calendarAuthorityIdentityRuntime{identity: "google:subject-a"}
	fixture.console.mu.Lock()
	record := fixture.console.state.Connections[connectionID]
	record.Scope = map[string]any{"calendar_id": "primary"}
	record.Incarnation = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"
	record.Epoch = 7
	record.Credential = "calendar-test-credential"
	record.ProviderIdentity = "google:subject-a"
	record.IdentityUnverified = false
	fixture.console.state.Connections[connectionID] = record
	fixture.console.mu.Unlock()
	principal := authorization.Principal{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true}
	if err := fixture.console.WithCurrentSource(principal, authorization.SourceReference{ConnectorID: "calendar.google", ConnectionID: connectionID, ExecutionOwner: fixture.console.state.ExecutionOwnerID, Incarnation: record.Incarnation, Epoch: record.Epoch}, func(authorization.SourceSnapshot) error { return nil }); err != nil {
		t.Fatalf("direct source fence: %v", err)
	}
	response := fixture.call(http.MethodPost, "/v1/authority/calendar/source", map[string]any{
		"connector_id": "calendar.google", "connection_id": connectionID, "resource": "primary",
	}, token)
	if response.Code != http.StatusOK {
		t.Fatalf("source preview: %d %s", response.Code, response.Body.String())
	}
	var value map[string]any
	if err := json.Unmarshal(response.Body.Bytes(), &value); err != nil {
		t.Fatal(err)
	}
	for _, field := range []string{"descriptor_b64url", "producer_signature", "audience", "execution_owner"} {
		if _, ok := value[field].(string); !ok {
			t.Fatalf("missing source preview field %q: %#v", field, value)
		}
	}
	descriptor, err := base64.RawURLEncoding.DecodeString(value["descriptor_b64url"].(string))
	if err != nil {
		t.Fatal(err)
	}
	signature, err := base64.RawURLEncoding.DecodeString(value["producer_signature"].(string))
	if err != nil {
		t.Fatal(err)
	}
	metadata, err := fixture.console.producerMetadata()
	if err != nil {
		t.Fatal(err)
	}
	publicKey, err := base64.RawURLEncoding.DecodeString(metadata["public_key"].(string))
	if err != nil {
		t.Fatal(err)
	}
	signed := append([]byte("floe.remote.producer.v1\x00"), descriptor...)
	if !ed25519.Verify(ed25519.PublicKey(publicKey), signed, signature) {
		t.Fatal("source descriptor signature did not verify")
	}
	var fields map[string]any
	if err := json.Unmarshal(descriptor, &fields); err != nil || fields["provider_identity"] != "google:subject-a" || fields["epoch"] != float64(7) {
		t.Fatalf("unexpected source descriptor: %s", descriptor)
	}
	fixture.console.mu.Lock()
	record = fixture.console.state.Connections[connectionID]
	record.ProviderIdentity = "google:subject-b"
	record.Epoch = 8
	fixture.console.state.Connections[connectionID] = record
	fixture.console.mu.Unlock()
	fixture.console.calendarAuth = &calendarAuthorityIdentityRuntime{identity: "google:subject-b"}
	replaced := fixture.call(http.MethodPost, "/v1/authority/calendar/source", map[string]any{
		"connector_id": "calendar.google", "connection_id": connectionID, "resource": "primary",
	}, token)
	if replaced.Code != http.StatusOK {
		t.Fatalf("replacement source preview: %d %s", replaced.Code, replaced.Body.String())
	}
	var replacement map[string]any
	if err := json.Unmarshal(replaced.Body.Bytes(), &replacement); err != nil {
		t.Fatal(err)
	}
	if replacement["descriptor_b64url"] == value["descriptor_b64url"] {
		t.Fatal("source replacement reused the old signed descriptor")
	}
	replacementDescriptor, err := base64.RawURLEncoding.DecodeString(replacement["descriptor_b64url"].(string))
	if err != nil {
		t.Fatal(err)
	}
	var replacementFields map[string]any
	if err := json.Unmarshal(replacementDescriptor, &replacementFields); err != nil || replacementFields["provider_identity"] != "google:subject-b" || replacementFields["epoch"] != float64(8) {
		t.Fatalf("replacement descriptor did not carry new source authority: %s", replacementDescriptor)
	}
}

func TestCalendarAuthoritySignedAdmissionReadRelease(t *testing.T) {
	fixture := setup(t)
	clientID, token := fixture.pair()
	fixture.ownConnector("calendar.google")
	connectionID := fixtureConnectionID("calendar.google")
	identityRuntime := &calendarAuthorityIdentityRuntime{identity: "google:calendar-subject"}
	fixture.console.mu.Lock()
	fixture.console.calendarAuth = identityRuntime
	record := fixture.console.state.Connections[connectionID]
	record.Scope = map[string]any{"calendar_id": "primary"}
	record.Credential = "calendar-test-credential"
	record.Incarnation = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"
	record.Epoch = 1
	record.ProviderIdentity = identityRuntime.identity
	record.IdentityUnverified = false
	fixture.console.state.Connections[connectionID] = record
	fixture.console.mu.Unlock()
	fixture.console.calendars = map[string]connections.CalendarRuntime{connectionID: &fakeCalendarRuntime{
		snapshot: calendarSnapshot("calendar.google", "google_calendar"),
		view:     map[string]any{"schema_version": 1, "view_id": "calendar.timeline", "items": []any{}},
	}}
	privateKey := ed25519.NewKeyFromSeed([]byte("01234567890123456789012345678901"))
	keyID := "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
	principal := authorization.Principal{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true}
	metadata, err := fixture.console.producerMetadata()
	if err != nil {
		t.Fatal(err)
	}
	enrollment, challenge, err := fixture.console.authorizationEngine.BeginEnrollment(principal, keyID, privateKey.Public().(ed25519.PublicKey), metadata["audience"].(string))
	if err != nil {
		t.Fatal(err)
	}
	ownerMessage := append([]byte(authorization.SignatureDomain), challenge.Bytes...)
	if err := fixture.console.authorizationEngine.CompleteEnrollment(enrollment.ID, principal, authorization.Proof{ChallengeID: challenge.ID, KeyID: keyID, Signature: base64.RawURLEncoding.EncodeToString(ed25519.Sign(privateKey, ownerMessage))}); err != nil {
		t.Fatal(err)
	}
	if err := fixture.console.authorizationEngine.ApproveEnrollment(enrollment.ID, enrollment.Fingerprint, true); err != nil {
		t.Fatal(err)
	}
	start := time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC).UnixMilli()
	admitBody := map[string]any{
		"schema_version": 1, "connector_id": "calendar.google", "connection_id": connectionID,
		"connection_revision": 1, "resources": []string{"primary"},
		"policy":  map[string]any{"incarnation": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", "epoch": 1},
		"grant":   map[string]any{"id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc", "incarnation": "dddddddd-dddd-4ddd-8ddd-dddddddddddd", "epoch": 1},
		"purpose": "everyday_assistance", "consumer": "floe.builtin.schedule", "max_items": 25, "max_bytes": 65536,
		"query": map[string]any{"range_start_unix_ms": start, "range_end_unix_ms": start + int64(24*time.Hour/time.Millisecond), "cursor": "", "limit": 25},
	}
	admit := fixture.call(http.MethodPost, "/v1/views/calendar.timeline/admit", admitBody, token)
	if admit.Code != http.StatusOK {
		t.Fatalf("admit: %d %s", admit.Code, admit.Body.String())
	}
	var admitted map[string]any
	if err := json.Unmarshal(admit.Body.Bytes(), &admitted); err != nil {
		t.Fatal(err)
	}
	challengeBytes, _ := base64.RawURLEncoding.DecodeString(admitted["challenge_b64url"].(string))
	ownerMessage = append([]byte(authorization.SignatureDomain), challengeBytes...)
	proofBody := map[string]any{"schema_version": 1, "proof": map[string]any{"challenge_id": admitted["challenge_id"], "key_id": keyID, "signature": base64.RawURLEncoding.EncodeToString(ed25519.Sign(privateKey, ownerMessage))}}
	read := fixture.call(http.MethodPost, "/v1/views/calendar.timeline/read", proofBody, token)
	if read.Code != http.StatusOK {
		t.Fatalf("read: %d %s", read.Code, read.Body.String())
	}
	var released map[string]any
	if err := json.Unmarshal(read.Body.Bytes(), &released); err != nil {
		t.Fatal(err)
	}
	releaseBytes, _ := base64.RawURLEncoding.DecodeString(released["challenge_b64url"].(string))
	ownerMessage = append([]byte(authorization.SignatureDomain), releaseBytes...)
	releaseProof := map[string]any{"schema_version": 1, "proof": map[string]any{"challenge_id": released["challenge_id"], "key_id": keyID, "signature": base64.RawURLEncoding.EncodeToString(ed25519.Sign(privateKey, ownerMessage))}}
	result := fixture.call(http.MethodPost, "/v1/views/calendar.timeline/release", releaseProof, token)
	if result.Code != http.StatusOK || !strings.Contains(result.Body.String(), "calendar.timeline") {
		t.Fatalf("release: %d %s", result.Code, result.Body.String())
	}
}
