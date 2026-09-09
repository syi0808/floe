package inference

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"
	"time"
)

type Config struct {
	Targets map[string]Target `json:"targets"`
	Routes  map[string]Route  `json:"routes"`
}

type Route struct {
	Target          string `json:"target"`
	ReasoningEffort string `json:"reasoning_effort,omitempty"`
}

type Target struct {
	Provider  string `json:"provider"`
	BaseURL   string `json:"base_url"`
	Model     string `json:"model"`
	APIKeyEnv string `json:"api_key_env,omitempty"`
}

type CodexClient interface {
	Ready() bool
	ReplayIdentity() string
	Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error)
}

type Request struct {
	ProviderIdentity string          `json:"-"`
	Agent            bool            `json:"-"`
	SchemaVersion    int             `json:"schema_version"`
	Purpose          string          `json:"purpose,omitempty"`
	DataClasses      []string        `json:"data_classes,omitempty"`
	AllowExternal    bool            `json:"allow_external"`
	Instructions     string          `json:"instructions"`
	Input            json.RawMessage `json:"input"`
	OutputSchema     json.RawMessage `json:"output_schema,omitempty"`
	ReplayOf         string          `json:"replay_of,omitempty"`
}

type Gateway struct {
	tokenHash [32]byte
	targets   map[string]*provider
	routes    map[string]route
	active    chan struct{}
	timeout   time.Duration
	audit     *auditLog
}

var targetID = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)

type route struct {
	provider *provider
	effort   string
}

func New(config Config, token string, lookup func(string) string, codex ...CodexClient) (*Gateway, error) {
	if len(token) < 32 || strings.ContainsAny(token, "\r\n ") || len(config.Targets) > 32 || len(config.Routes) > 8 {
		return nil, errors.New("invalid inference configuration")
	}
	gateway := &Gateway{tokenHash: sha256.Sum256([]byte(token)), targets: make(map[string]*provider), routes: make(map[string]route), active: make(chan struct{}, 4), timeout: 40 * time.Second, audit: newAuditLog(256)}
	var codexClient CodexClient
	if len(codex) > 0 {
		codexClient = codex[0]
	}
	for identifier, target := range config.Targets {
		if !targetID.MatchString(identifier) {
			return nil, errors.New("invalid target ID")
		}
		adapter, err := newProvider(target, lookup, codexClient)
		if err != nil {
			return nil, err
		}
		gateway.targets[identifier] = adapter
	}
	for class, configured := range config.Routes {
		adapter, exists := gateway.targets[configured.Target]
		if !ValidClass(class) || !exists || !validEffort(configured.ReasoningEffort) {
			return nil, errors.New("invalid inference route")
		}
		gateway.routes[class] = route{provider: adapter, effort: configured.ReasoningEffort}
	}
	return gateway, nil
}

func validEffort(value string) bool {
	return value == "" || value == "low" || value == "medium" || value == "high" || value == "xhigh" || value == "max" || value == "ultra"
}

func ValidClass(value string) bool {
	return value == "fast" || value == "balanced" || value == "high_effort"
}

func ValidPurpose(value string) bool {
	return value == "quick_response" || value == "everyday_assistance" || value == "deep_work"
}

func classForPurpose(value string) string {
	switch value {
	case "quick_response":
		return "fast"
	case "everyday_assistance":
		return "balanced"
	case "deep_work":
		return "high_effort"
	default:
		return ""
	}
}

func (gateway *Gateway) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
	writer.Header().Set("Content-Type", "application/json")
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("X-Content-Type-Options", "nosniff")
	if request.Header.Get("Origin") != "" {
		writeError(writer, http.StatusForbidden, "unauthorized")
		return
	}
	supplied := strings.TrimPrefix(request.Header.Get("Authorization"), "Bearer ")
	hash := sha256.Sum256([]byte(supplied))
	if !strings.HasPrefix(request.Header.Get("Authorization"), "Bearer ") || subtle.ConstantTimeCompare(hash[:], gateway.tokenHash[:]) != 1 {
		writeError(writer, http.StatusUnauthorized, "unauthorized")
		return
	}
	if request.Method == http.MethodGet && request.URL.Path == "/v1/inference-purposes" {
		purposes := make(map[string]any, 3)
		for _, purpose := range []string{"quick_response", "everyday_assistance", "deep_work"} {
			configured, available := gateway.routes[classForPurpose(purpose)]
			value := map[string]any{"available": available, "requires_external_consent": available && configured.provider.external}
			if available {
				placement, recipient := configured.provider.disclosure()
				value["placement"] = placement
				if recipient != "" {
					value["recipient"] = recipient
				}
			}
			purposes[purpose] = value
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "purposes": purposes})
		return
	}
	if request.Method == http.MethodGet && request.URL.Path == "/v1/traces" {
		_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "traces": gateway.audit.list(20)})
		return
	}
	if request.Method == http.MethodGet && strings.HasPrefix(request.URL.Path, "/v1/traces/") {
		identifier := strings.TrimPrefix(request.URL.Path, "/v1/traces/")
		record, exists := gateway.audit.get(identifier)
		if !exists {
			writeError(writer, http.StatusNotFound, "trace_not_found")
			return
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "trace": record})
		return
	}
	if request.Method != http.MethodPost || (request.URL.Path != "/v1/generate" && request.URL.Path != "/v1/agent") {
		writeError(writer, http.StatusNotFound, "not_found")
		return
	}
	var input Request
	input.Agent = request.URL.Path == "/v1/agent"
	decoder := json.NewDecoder(http.MaxBytesReader(writer, request.Body, 98304))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&input) != nil || decoder.Decode(new(any)) != io.EOF || !validRequest(input) {
		writeError(writer, http.StatusBadRequest, "validation")
		return
	}
	class := classForPurpose(input.Purpose)
	configured, exists := gateway.routes[class]
	if !exists {
		writeError(writer, http.StatusBadRequest, "route_unavailable")
		return
	}
	if input.ReplayOf != "" {
		original, found := gateway.audit.get(input.ReplayOf)
		if !found || original.Outcome != "completed" || original.RequestDigest != requestDigest(input) {
			writeError(writer, http.StatusConflict, "replay_mismatch")
			return
		}
	}
	adapter := configured.provider
	replaySource := ""
	if input.Agent {
		if adapter.codex != nil {
			input.ProviderIdentity = adapter.codex.ReplayIdentity()
		}
		replaySource = gateway.replaySource(configured, input.Purpose, input.ProviderIdentity)
		var agentInput AgentInput
		if json.Unmarshal(input.Input, &agentInput) != nil {
			writeError(writer, http.StatusBadRequest, "validation")
			return
		}
		if agentInput.ReplaySource != "" && agentInput.ReplaySource != replaySource {
			writeError(writer, http.StatusConflict, "replay_source_mismatch")
			return
		}
	}
	if adapter.external && !input.AllowExternal {
		writeError(writer, http.StatusForbidden, "external_transfer_denied")
		return
	}
	select {
	case gateway.active <- struct{}{}:
		defer func() { <-gateway.active }()
	default:
		writeError(writer, http.StatusTooManyRequests, "model_busy")
		return
	}
	ctx, cancel := context.WithTimeout(request.Context(), gateway.timeout)
	defer cancel()
	output, err := adapter.generate(ctx, input, configured.effort)
	traceID := newTraceID()
	placement := "server_local"
	if adapter.external {
		placement = "remote"
	}
	if err != nil {
		code := "model_unavailable"
		if errors.Is(err, context.DeadlineExceeded) || errors.Is(ctx.Err(), context.DeadlineExceeded) {
			code = "model_timeout"
		} else if errors.Is(err, errInvalidOutput) {
			code = "invalid_proposal"
		}
		gateway.audit.add(newAuditRecord(traceID, input, placement, code, ""))
		writeErrorWithTrace(writer, http.StatusBadGateway, code, traceID)
		return
	}
	gateway.audit.add(newAuditRecord(traceID, input, placement, "completed", output))
	_ = json.NewEncoder(writer).Encode(map[string]any{
		"schema_version": input.SchemaVersion,
		"purpose":        input.Purpose,
		"output":         output,
		"routing":        map[string]any{"placement": placement, "external_transfer": adapter.external, "replay_source": replaySource},
		"trace_id":       traceID,
	})
}

func validRequest(request Request) bool {
	if request.Agent {
		return request.SchemaVersion == 1 && ValidPurpose(request.Purpose) && validDataClasses(request.DataClasses) && request.ReplayOf == "" && len(request.OutputSchema) == 0 && len(request.Instructions) > 0 && len(request.Instructions) <= 8192 && len(request.Input) <= 32768 && validAgentInput(request.Input)
	}
	var schema map[string]any
	validRoute := request.SchemaVersion == 1 && ValidPurpose(request.Purpose) && validDataClasses(request.DataClasses)
	validReplay := request.ReplayOf == "" || len(request.ReplayOf) == 32 && strings.IndexFunc(request.ReplayOf, func(value rune) bool {
		return value < '0' || value > '9' && value < 'a' || value > 'f'
	}) == -1
	return validRoute && validReplay &&
		len(request.Instructions) > 0 && len(request.Instructions) <= 8192 &&
		len(request.Input) > 0 && len(request.Input) <= 32768 && json.Valid(request.Input) &&
		len(request.OutputSchema) <= 32768 && json.Unmarshal(request.OutputSchema, &schema) == nil && schema["type"] == "object"
}

func validDataClasses(values []string) bool {
	if len(values) == 0 || len(values) > 4 {
		return false
	}
	seen := map[string]bool{}
	for _, value := range values {
		if seen[value] || value != "synthetic" && value != "personal" && value != "highly_sensitive" {
			return false
		}
		seen[value] = true
	}
	return true
}

func newTraceID() string {
	value := make([]byte, 16)
	if _, err := rand.Read(value); err != nil {
		panic("crypto/rand unavailable")
	}
	return fmt.Sprintf("%x", value)
}

func writeError(writer http.ResponseWriter, status int, code string) {
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "error": map[string]string{"code": code}})
}

func writeErrorWithTrace(writer http.ResponseWriter, status int, code, traceID string) {
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "error": map[string]string{"code": code}, "trace_id": traceID})
}
