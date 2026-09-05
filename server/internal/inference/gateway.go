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
	Generate(context.Context, string, string, string, json.RawMessage, json.RawMessage) (string, error)
}

type Request struct {
	SchemaVersion  int             `json:"schema_version"`
	InferenceClass string          `json:"inference_class"`
	AllowExternal  bool            `json:"allow_external"`
	Instructions   string          `json:"instructions"`
	Input          json.RawMessage `json:"input"`
	OutputSchema   json.RawMessage `json:"output_schema"`
}

type Gateway struct {
	tokenHash [32]byte
	targets   map[string]*provider
	routes    map[string]route
	active    chan struct{}
	timeout   time.Duration
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
	gateway := &Gateway{tokenHash: sha256.Sum256([]byte(token)), targets: make(map[string]*provider), routes: make(map[string]route), active: make(chan struct{}, 4), timeout: 40 * time.Second}
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
	return value == "" || value == "low" || value == "medium" || value == "high" || value == "xhigh"
}

func ValidClass(value string) bool {
	return value == "fast" || value == "balanced" || value == "high_effort"
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
	if request.Method == http.MethodGet && request.URL.Path == "/v1/inference-classes" {
		classes := make(map[string]any, len(gateway.routes))
		for class, configured := range gateway.routes {
			classes[class] = map[string]any{"available": true, "requires_external_consent": configured.provider.external}
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "inference_classes": classes})
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
	configured, exists := gateway.routes[input.InferenceClass]
	if !exists {
		writeError(writer, http.StatusBadRequest, "inference_class_unavailable")
		return
	}
	adapter := configured.provider
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
	_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "inference_class": input.InferenceClass, "output": output})
}

func validRequest(request Request) bool {
	var schema map[string]any
	return request.SchemaVersion == 1 && ValidClass(request.InferenceClass) &&
		len(request.Instructions) > 0 && len(request.Instructions) <= 8192 &&
		len(request.Input) > 0 && len(request.Input) <= 32768 && json.Valid(request.Input) &&
		len(request.OutputSchema) <= 32768 && json.Unmarshal(request.OutputSchema, &schema) == nil && schema["type"] == "object"
}

func writeError(writer http.ResponseWriter, status int, code string) {
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(map[string]any{"schema_version": 1, "error": map[string]string{"code": code}})
}
