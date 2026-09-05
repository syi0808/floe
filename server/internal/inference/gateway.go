package inference

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"regexp"
	"strings"
	"time"
)

type Config struct {
	Targets map[string]Target `json:"targets"`
}

type Target struct {
	Provider  string `json:"provider"`
	BaseURL   string `json:"base_url"`
	Model     string `json:"model"`
	APIKeyEnv string `json:"api_key_env,omitempty"`
}

type CodexClient interface {
	Ready() bool
	Generate(context.Context, string, string, json.RawMessage, json.RawMessage) (string, error)
}

type Request struct {
	SchemaVersion int             `json:"schema_version"`
	Target        string          `json:"target"`
	AllowExternal bool            `json:"allow_external"`
	Instructions  string          `json:"instructions"`
	Input         json.RawMessage `json:"input"`
	OutputSchema  json.RawMessage `json:"output_schema"`
}

type Gateway struct {
	tokenHash [32]byte
	targets   map[string]*provider
	active    chan struct{}
	timeout   time.Duration
}

var targetID = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)

func New(config Config, token string, lookup func(string) string, codex ...CodexClient) (*Gateway, error) {
	if len(token) < 32 || strings.ContainsAny(token, "\r\n ") || len(config.Targets) > 32 {
		return nil, errors.New("invalid inference configuration")
	}
	gateway := &Gateway{tokenHash: sha256.Sum256([]byte(token)), targets: make(map[string]*provider), active: make(chan struct{}, 4), timeout: 40 * time.Second}
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
	return gateway, nil
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
	if request.Method == http.MethodGet && request.URL.Path == "/v1/targets" {
		targets := make(map[string]any, len(gateway.targets))
		for identifier, adapter := range gateway.targets {
			targets[identifier] = map[string]any{"model": adapter.target.Model, "provider": adapter.target.Provider, "requires_external_consent": adapter.external}
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "targets": targets})
		return
	}
	if request.Method != http.MethodPost || request.URL.Path != "/v1/generate" {
		writeError(writer, http.StatusNotFound, "not_found")
		return
	}
	var input Request
	decoder := json.NewDecoder(http.MaxBytesReader(writer, request.Body, 98304))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&input) != nil || decoder.Decode(new(any)) != io.EOF || !validRequest(input) {
		writeError(writer, http.StatusBadRequest, "validation")
		return
	}
	adapter, exists := gateway.targets[input.Target]
	if !exists {
		writeError(writer, http.StatusBadRequest, "unknown_target")
		return
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
	output, err := adapter.generate(ctx, input)
	if err != nil {
		code := "model_unavailable"
		if errors.Is(err, context.DeadlineExceeded) || errors.Is(ctx.Err(), context.DeadlineExceeded) {
			code = "model_timeout"
		} else if errors.Is(err, errInvalidOutput) {
			code = "invalid_proposal"
		}
		writeError(writer, http.StatusBadGateway, code)
		return
	}
	_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "target": input.Target, "model": adapter.target.Model, "output": output})
}

func validRequest(request Request) bool {
	var schema map[string]any
	return request.SchemaVersion == 1 && targetID.MatchString(request.Target) &&
		len(request.Instructions) > 0 && len(request.Instructions) <= 8192 &&
		len(request.Input) > 0 && len(request.Input) <= 32768 && json.Valid(request.Input) &&
		len(request.OutputSchema) <= 32768 && json.Unmarshal(request.OutputSchema, &schema) == nil && schema["type"] == "object"
}

func writeError(writer http.ResponseWriter, status int, code string) {
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "error": map[string]string{"code": code}})
}
