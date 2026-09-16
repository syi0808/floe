package httptransport

import (
	"crypto/ed25519"
	cryptorand "crypto/rand"
	"encoding/base64"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"floe/server/internal/authorization"
)

type authorityServiceStub struct {
	principal authorization.Principal
}

func (stub *authorityServiceStub) BeginEnrollment(principal authorization.Principal, keyID string, publicKey ed25519.PublicKey, audience string) (authorization.Enrollment, authorization.Challenge, error) {
	stub.principal = principal
	return authorization.Enrollment{ID: "enrollment", Fingerprint: "fingerprint", KeyID: keyID}, authorization.Challenge{ID: "challenge", Bytes: []byte("challenge"), BytesB64: "Y2hhbGxlbmdl", ExpiresAt: time.Unix(100, 0)}, nil
}

func (*authorityServiceStub) CompleteEnrollment(string, authorization.Principal, authorization.Proof) error {
	return nil
}

func (*authorityServiceStub) EnrollmentStatus(string, authorization.Principal) (authorization.EnrollmentStatus, error) {
	return authorization.EnrollmentStatus{}, nil
}

func (*authorityServiceStub) PendingEnrollments() []authorization.EnrollmentStatus { return nil }
func (*authorityServiceStub) ActiveIssuers() []authorization.EnrollmentStatus      { return nil }
func (*authorityServiceStub) ApproveEnrollment(string, string, bool) error         { return nil }
func (*authorityServiceStub) RevokeIssuer(string) error                            { return nil }

func TestAuthorityTransportPreservesEnrollmentWireAndPrincipal(t *testing.T) {
	stub := &authorityServiceStub{}
	publicKey, _, err := ed25519.GenerateKey(cryptorand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	handler := AuthorityHandler{
		Service: stub,
		ProducerMetadata: func() (map[string]any, error) {
			return map[string]any{"audience": "floe.server:test", "key_id": "producer", "public_key": "producer-public", "fingerprint": "producer-fingerprint"}, nil
		},
		Sign: func(value []byte) []byte { return append([]byte("signed:"), value...) },
	}
	body := `{"key_id":"key","public_key":"` + base64.RawURLEncoding.EncodeToString(publicKey) + `","audience":"ignored"}`
	request := httptest.NewRequest(http.MethodPost, "/v1/authority/enrollment/begin", strings.NewReader(body))
	request.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	principal := authorization.Principal{ClientID: "client", PersonID: "person", DeviceID: "device", Authenticated: true}
	if !handler.ServeClient(response, request, principal) {
		t.Fatal("request was not handled")
	}
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", response.Code, http.StatusOK)
	}
	if stub.principal != principal {
		t.Fatalf("principal = %+v, want %+v", stub.principal, principal)
	}
	for _, expected := range []string{`"enrollment_id":"enrollment"`, `"challenge_b64url":"Y2hhbGxlbmdl"`, `"producer_signature":"c2lnbmVkOmNoYWxsZW5nZQ"`} {
		if !strings.Contains(response.Body.String(), expected) {
			t.Fatalf("response missing %s: %s", expected, response.Body.String())
		}
	}
}

func TestStrictJSONRejectsDuplicateAndCaseAlias(t *testing.T) {
	for _, input := range []string{`{"key":"one","key":"two"}`, `{"Key":"value"}`} {
		if StrictJSON([]byte(input)) {
			t.Fatalf("StrictJSON accepted %s", input)
		}
	}
	if !StrictJSON([]byte(`{"key":"value","nested":{"item":[1,true,null]}}`)) {
		t.Fatal("StrictJSON rejected valid bounded JSON")
	}
}
