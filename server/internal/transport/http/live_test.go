package httptransport

import (
	"context"
	"encoding/json"
	"floe/server/internal/inference"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

type liveObservationAccount struct{}

func (liveObservationAccount) Ready(context.Context) error { return nil }
func (liveObservationAccount) ReplayIdentity() string {
	return "synthetic-account-correlation-never-public"
}
func (liveObservationAccount) ProtocolCapabilities() []string { return []string{} }
func (liveObservationAccount) ModelIdentity() inference.ModelIdentity {
	return inference.ModelIdentity{ProviderID: "codex_oauth", ModelID: "gpt-live-1-codex", Endpoint: "https://chatgpt.com/backend-api/codex"}
}
func (liveObservationAccount) BudgetOverride() *inference.ModelBudgetOverride { return nil }
func (a liveObservationAccount) LiveProfile() (inference.LiveProfile, bool) {
	return inference.LiveProfile{ModelIdentity: a.ModelIdentity(), Capabilities: []string{"duplex_audio", "final_input_transcript", "final_output_transcript", "client_delegation", "context_append"}, Limits: inference.LiveTransportLimits{InitialContextBytes: 2048, AppendBytes: 500, LocalSessionDurationMS: 60000}, ProtocolSource: "synthetic-fixture", Experimental: true}, true
}
func (liveObservationAccount) PrepareLiveTransport(context.Context, string) (inference.PreparedLiveTransport, error) {
	panic("GET cannot prepare live transport")
}

type noLiveGeneration struct{}

func (noLiveGeneration) InvokeAgent(context.Context, inference.ResolvedModelTarget, inference.AgentInvocation) (inference.AgentResult, error) {
	panic("GET cannot generate")
}
func (noLiveGeneration) InvokeStructured(context.Context, inference.ResolvedModelTarget, inference.StructuredInvocation) (inference.StructuredResult, error) {
	panic("GET cannot generate")
}
func TestPairedLiveObservationUsesExistingTrustAndDoesNotExposeAccount(t *testing.T) {
	f := newConsoleFixture(t)
	cookie, csrf := f.login(t)
	bearer := completeSyntheticPairing(t, f, cookie, csrf)
	if r := f.inferenceRequest(http.MethodGet, "/v1/live-model", nil, ""); r.Code != http.StatusUnauthorized {
		t.Fatal("unpaired observation accepted", r.Code)
	}
	r := f.inferenceRequest(http.MethodGet, "/v1/live-model", nil, bearer)
	var missing livePurposeDTO
	if r.Code != 200 || json.Unmarshal(r.Body.Bytes(), &missing) != nil || missing.Status != "not_configured" || missing.Profile != nil {
		t.Fatal("invalid explicit absence", r.Code)
	}
	err := f.handler.Inference.Service.Configure(inference.InferenceConfig{Routes: map[inference.Purpose]inference.PurposeRoute{inference.LivePresentation: {TargetID: "live", Enabled: true}}}, map[string]inference.ModelAccount{"live": liveObservationAccount{}}, noLiveGeneration{})
	if err != nil {
		t.Fatal(err)
	}
	operator, err := f.trust.AuthenticateOperatorSession(context.Background(), cookie.Value, csrf, true)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = f.handler.Inference.Service.ProbeTarget(context.Background(), operator, "live"); err == nil {
		t.Fatal("live-only transport entered generation diagnostic")
	}
	r = f.inferenceRequest(http.MethodGet, "/v1/live-model", nil, bearer)
	var observed livePurposeDTO
	if r.Code != 200 || json.Unmarshal(r.Body.Bytes(), &observed) != nil || observed.Status != "available" || observed.Profile == nil || !observed.Profile.Experimental || len(observed.CapabilityRevision) != 64 {
		t.Fatal("invalid live projection", r.Code, r.Body.String())
	}
	if strings.Contains(r.Body.String(), "synthetic-account-correlation") || strings.Contains(r.Body.String(), "token") {
		t.Fatal("credential/account material exposed")
	}
	for _, target := range []string{"/v1/live-model?unexpected=1"} {
		if r = f.inferenceRequest(http.MethodGet, target, nil, bearer); r.Code != http.StatusBadRequest {
			t.Fatal("query accepted")
		}
	}
	if r = f.inferenceRequest(http.MethodPost, "/v1/live-model", nil, bearer); r.Code != http.StatusMethodNotAllowed {
		t.Fatal("write method accepted")
	}
	req := httptest.NewRequest(http.MethodGet, "/v1/live-model", nil)
	req.Host = f.address
	req.Header.Set("Authorization", "Bearer "+bearer)
	req.Header.Set("Origin", "http://"+f.address)
	response := httptest.NewRecorder()
	f.handler.ServeHTTP(response, req)
	if response.Code != http.StatusForbidden {
		t.Fatal("browser-origin paired route accepted")
	}
}
