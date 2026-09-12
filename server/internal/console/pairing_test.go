package console

import (
	"crypto/ed25519"
	"encoding/base64"
	"net/http"
	"path/filepath"
	"strings"
	"testing"

	"floe/server/internal/authorization"
)

func pairingConfirmBody(test *testing.T, started map[string]any) map[string]any {
	test.Helper()
	keyID := started["issuer"].(map[string]any)["key_id"].(string)
	challenge, err := base64.RawURLEncoding.DecodeString(started["challenge_b64url"].(string))
	if err != nil {
		test.Fatal(err)
	}
	privateKey := pairIssuerPrivateKey(keyID)
	return map[string]any{
		"schema_version": 1,
		"pairing_id":     started["pairing_id"],
		"proof":          started["proof"],
		"challenge_id":   started["challenge_id"],
		"key_id":         keyID,
		"signature": base64.RawURLEncoding.EncodeToString(ed25519.Sign(privateKey,
			append([]byte(authorization.SignatureDomain), challenge...))),
	}
}

func TestPairingRequiresLocalConfirmationAndExactFingerprint(t *testing.T) {
	fixture := setup(t)
	started := fixture.value(fixture.call(http.MethodPost, "/pair/start", pairStartBody(fixturePersonID, fixtureDeviceID), ""))
	issuer := started["issuer"].(map[string]any)
	approve := fixture.call(http.MethodPost, "/manage/api/pair/approve", map[string]any{
		"schema_version": 1, "pairing_id": started["pairing_id"], "issuer_fingerprint": issuer["fingerprint"],
	}, "")
	if approve.Code != http.StatusConflict || !strings.Contains(approve.Body.String(), "pairing_not_confirmed") {
		t.Fatalf("approval without local confirmation: %d %s", approve.Code, approve.Body.String())
	}
	fixture.console.mu.Lock()
	if len(fixture.console.state.Clients) != 0 || len(fixture.console.state.TrustedIssuers) != 0 {
		fixture.console.mu.Unlock()
		t.Fatal("unconfirmed pairing changed durable state")
	}
	fixture.console.mu.Unlock()

	confirm := fixture.call(http.MethodPost, "/pair/confirm", pairingConfirmBody(t, started), "")
	if confirm.Code != http.StatusOK || !strings.Contains(confirm.Body.String(), "local_confirmed") {
		t.Fatalf("local confirmation: %d %s", confirm.Code, confirm.Body.String())
	}
	wrong := fixture.call(http.MethodPost, "/manage/api/pair/approve", map[string]any{
		"schema_version": 1, "pairing_id": started["pairing_id"], "issuer_fingerprint": "wrong",
	}, "")
	if wrong.Code != http.StatusConflict || !strings.Contains(wrong.Body.String(), "fingerprint_mismatch") {
		t.Fatalf("wrong fingerprint approved: %d %s", wrong.Code, wrong.Body.String())
	}
	pending := fixture.value(fixture.call(http.MethodPost, "/pair/poll", map[string]any{"proof": started["proof"]}, ""))
	if pending["status"] != "local_confirmed" || pending["token"] != nil {
		t.Fatalf("wrong fingerprint issued credential: %#v", pending)
	}

	approved := fixture.value(fixture.call(http.MethodPost, "/manage/api/pair/approve", map[string]any{
		"schema_version": 1, "pairing_id": started["pairing_id"], "issuer_fingerprint": issuer["fingerprint"],
	}, ""))
	if approved["status"] != "approved" {
		t.Fatalf("approval status: %#v", approved)
	}
	credential := fixture.value(fixture.call(http.MethodPost, "/pair/poll", map[string]any{"proof": started["proof"]}, ""))
	if credential["token"] == nil || credential["client_id"] != started["pairing_id"] {
		t.Fatalf("approved pairing did not issue credential: %#v", credential)
	}
	fixture.console.mu.Lock()
	client := fixture.console.state.Clients[started["pairing_id"].(string)]
	trusted := fixture.console.state.TrustedIssuers[issuer["key_id"].(string)]
	producerFingerprint := fixture.console.producer.fingerprint()
	fixture.console.mu.Unlock()
	if client.TokenHash == "" || client.ProducerFingerprint != producerFingerprint || trusted.ClientID != client.ClientID {
		t.Fatalf("pairing activation was not bound atomically: client=%+v trusted=%+v", client, trusted)
	}
}

func TestPairingCredentialRemainsHiddenWhenActivationSaveFails(t *testing.T) {
	fixture := setup(t)
	started := fixture.value(fixture.call(http.MethodPost, "/pair/start", pairStartBody(fixturePersonID, fixtureDeviceID), ""))
	issuer := started["issuer"].(map[string]any)
	if response := fixture.call(http.MethodPost, "/pair/confirm", pairingConfirmBody(t, started), ""); response.Code != http.StatusOK {
		t.Fatalf("local confirmation: %d %s", response.Code, response.Body.String())
	}
	originalDirectory := fixture.console.directory
	fixture.console.directory = filepath.Join(originalDirectory, "missing", "state")
	failed := fixture.call(http.MethodPost, "/manage/api/pair/approve", map[string]any{
		"schema_version": 1, "pairing_id": started["pairing_id"], "issuer_fingerprint": issuer["fingerprint"],
	}, "")
	fixture.console.directory = originalDirectory
	if failed.Code != http.StatusConflict {
		t.Fatalf("failed activation status: %d %s", failed.Code, failed.Body.String())
	}
	pending := fixture.value(fixture.call(http.MethodPost, "/pair/poll", map[string]any{"proof": started["proof"]}, ""))
	if pending["status"] != "local_confirmed" || pending["token"] != nil {
		t.Fatalf("failed activation exposed credential: %#v", pending)
	}
	fixture.console.mu.Lock()
	if len(fixture.console.state.Clients) != 0 || len(fixture.console.state.TrustedIssuers) != 0 {
		fixture.console.mu.Unlock()
		t.Fatal("failed activation changed durable state")
	}
	fixture.console.mu.Unlock()
	if response := fixture.call(http.MethodPost, "/manage/api/pair/approve", map[string]any{
		"schema_version": 1, "pairing_id": started["pairing_id"], "issuer_fingerprint": issuer["fingerprint"],
	}, ""); response.Code != http.StatusOK {
		t.Fatalf("retry activation: %d %s", response.Code, response.Body.String())
	}
}
