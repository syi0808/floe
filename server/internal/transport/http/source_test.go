package httptransport

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/trust"
	"floe/server/internal/views"
)

type sourceHTTPEnforcement struct{}

func (sourceHTTPEnforcement) IssueViewAdmission(trust.Principal, sourcecontract.Snapshot, string, string, string, string, uint64, []string, [32]byte, sourcecontract.Bounds) (string, []byte, time.Time, error) {
	return "", nil, time.Time{}, nil
}
func (sourceHTTPEnforcement) CancelViewAdmission(string) {}
func (sourceHTTPEnforcement) ClaimViewAdmission(trust.Principal, sourcecontract.ID, trust.Proof) (string, sourcecontract.Snapshot, [32]byte, sourcecontract.Bounds, error) {
	return "", sourcecontract.Snapshot{}, [32]byte{}, sourcecontract.Bounds{}, nil
}
func (sourceHTTPEnforcement) StageViewResult(string, trust.Principal, []byte, uint32) (string, []byte, time.Time, error) {
	return "", nil, time.Time{}, nil
}
func (sourceHTTPEnforcement) CancelViewRelease(string) {}
func (sourceHTTPEnforcement) ClaimViewRelease(context.Context, trust.Principal, sourcecontract.ID, trust.Proof) ([]byte, error) {
	return nil, nil
}

type sourceHTTPTrust struct{}

func (sourceHTTPTrust) ProducerMetadata() (trust.ProducerMetadata, error) {
	return trust.ProducerMetadata{SchemaVersion: 1, InstanceID: "instance", ExecutionOwner: "owner", Audience: "audience", KeyID: "key", PublicKey: "public", Fingerprint: "fingerprint"}, nil
}
func (sourceHTTPTrust) SignProducerChallenge([]byte) ([]byte, error) { return []byte("signature"), nil }

type sourceHTTPResolver struct{ snapshot sourcecontract.Snapshot }

func (resolver sourceHTTPResolver) ResolveSource(_ context.Context, _ trust.Principal, _ views.SourceTarget) (views.ResolvedSource, error) {
	return views.ResolvedSource{Snapshot: sourcecontract.Clone(resolver.snapshot), Limits: views.Bounds{MaxItems: 128, MaxBytes: 1 << 20}}, nil
}
func (resolver sourceHTTPResolver) PreflightSource(_ context.Context, _ trust.Principal, expected sourcecontract.Snapshot) error {
	if expected.ConnectionID != resolver.snapshot.ConnectionID || expected.Epoch != resolver.snapshot.Epoch {
		return views.ErrInvalid
	}
	return nil
}

func TestSourcePreviewKeepsViewsRouteAndPublicEnvelope(t *testing.T) {
	connectionID := trust.NewID()
	resource := string(views.Calendar) + ":" + connectionID
	snapshot := sourcecontract.Snapshot{
		SourceReference:    sourcecontract.SourceReference{ConnectorID: "calendar-provider", ConnectionID: connectionID, ExecutionOwner: "owner", Incarnation: "incarnation", Epoch: 1},
		ConnectionRevision: 7, PersonID: "person", ProviderIdentity: "provider:account", IdentityGeneration: 1,
		Resources: []string{resource}, Active: true,
		Descriptor: sourcecontract.Descriptor{SchemaVersion: 1, ID: string(views.Calendar), Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 60_000, MaxItems: 128, MaxBytes: 1 << 20, ProvenanceRequired: true},
	}
	service, err := views.NewService(sourceHTTPEnforcement{}, sourceHTTPTrust{}, sourceHTTPResolver{snapshot})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(service.Close)
	body, err := json.Marshal(sourcePreviewRequest{ConnectorID: snapshot.ConnectorID, ConnectionID: connectionID, Resource: resource})
	if err != nil {
		t.Fatal(err)
	}
	request := httptest.NewRequest(http.MethodPost, "/v1/views/"+string(views.Calendar)+"/source-preview", strings.NewReader(string(body)))
	request.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	serveSource(response, request, trust.Principal{}, service)
	if response.Code != http.StatusOK {
		t.Fatalf("legacy View route returned %d: %s", response.Code, response.Body.String())
	}
	var wire struct {
		trust.ProducerMetadata
		Descriptor         string   `json:"descriptor_b64url"`
		Signature          string   `json:"producer_signature"`
		ExpiresAtUnixMS    int64    `json:"expires_at_unix_ms"`
		ConnectionRevision uint64   `json:"connection_revision"`
		SourceResources    []string `json:"source_resources"`
	}
	if err = json.Unmarshal(response.Body.Bytes(), &wire); err != nil {
		t.Fatalf("decode source preview envelope: %v", err)
	}
	if wire.Audience != "audience" || wire.Descriptor == "" || wire.Signature == "" || wire.ExpiresAtUnixMS == 0 || wire.ConnectionRevision != 7 || len(wire.SourceResources) != 1 || wire.SourceResources[0] != resource {
		t.Fatalf("source preview wire contract changed: %#v", wire)
	}

	missingAction := httptest.NewRequest(http.MethodPost, "/v1/views/"+string(views.Calendar), nil)
	missingResponse := httptest.NewRecorder()
	serveSource(missingResponse, missingAction, trust.Principal{}, service)
	if missingResponse.Code != http.StatusBadRequest || !strings.Contains(missingResponse.Body.String(), "admission_required") {
		t.Fatalf("incomplete legacy View route returned %d: %s", missingResponse.Code, missingResponse.Body.String())
	}
}
