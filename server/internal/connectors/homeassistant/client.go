package homeassistant

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

const maxEntities = 16

var allowedEntity = regexp.MustCompile(`^(sensor|binary_sensor|climate|light|switch)\.[a-z0-9_]{1,128}$`)

var (
	ErrInvalidInput      = errors.New("invalid home assistant input")
	ErrCredentialExpired = errors.New("home assistant credential expired")
	ErrRateLimited       = errors.New("home assistant rate limited")
	ErrUnavailable       = errors.New("home assistant unavailable")
	ErrInvalidResponse   = errors.New("invalid home assistant response")
)

type TokenSource interface {
	Token(context.Context) (string, error)
}

type Client struct {
	tokens       TokenSource
	baseURL      string
	connectionID string
	http         *http.Client
}

type stateResponse struct {
	EntityID    string `json:"entity_id"`
	State       string `json:"state"`
	LastUpdated string `json:"last_updated"`
	Attributes  struct {
		FriendlyName string `json:"friendly_name"`
	} `json:"attributes"`
}

type LogisticsItem struct {
	EvidenceHandle string `json:"evidence_handle"`
	Kind           string `json:"kind"`
	Summary        string `json:"summary"`
	Status         string `json:"status"`
	NeedsAttention bool   `json:"needs_attention"`
}

type LogisticsView struct {
	SchemaVersion    int             `json:"schema_version"`
	ViewID           string          `json:"view_id"`
	SourceHandle     string          `json:"source_handle"`
	ObservedAtUnixMS int64           `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64           `json:"expires_at_unix_ms"`
	CoverageComplete bool            `json:"coverage_complete"`
	Items            []LogisticsItem `json:"items"`
}

func New(tokens TokenSource, baseURL, connectionID string) (*Client, error) {
	if tokens == nil || !validIdentifier(connectionID) {
		return nil, ErrInvalidInput
	}
	endpoint, err := url.Parse(baseURL)
	if err != nil || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.Fragment != "" || strings.TrimSuffix(endpoint.Path, "/") != "" {
		return nil, ErrInvalidInput
	}
	loopback := endpoint.Scheme == "http" && (endpoint.Hostname() == "127.0.0.1" || endpoint.Hostname() == "localhost")
	if endpoint.Scheme != "https" && !loopback {
		return nil, ErrInvalidInput
	}
	return &Client{
		tokens:       tokens,
		baseURL:      strings.TrimSuffix(endpoint.String(), "/"),
		connectionID: connectionID,
		http: &http.Client{
			Timeout:   10 * time.Second,
			Transport: &http.Transport{Proxy: nil},
			CheckRedirect: func(*http.Request, []*http.Request) error {
				return http.ErrUseLastResponse
			},
		},
	}, nil
}

func (client *Client) Logistics(ctx context.Context, entities []string, now time.Time) (LogisticsView, error) {
	if len(entities) == 0 || len(entities) > maxEntities {
		return LogisticsView{}, ErrInvalidInput
	}
	seen := map[string]bool{}
	for _, entity := range entities {
		if !allowedEntity.MatchString(entity) || seen[entity] {
			return LogisticsView{}, ErrInvalidInput
		}
		seen[entity] = true
	}
	view := LogisticsView{
		SchemaVersion:    1,
		ViewID:           "life.logistics",
		SourceHandle:     handle("home", client.connectionID+":"+now.UTC().Format("2006-01-02T15:04")),
		ObservedAtUnixMS: now.UnixMilli(),
		ExpiresAtUnixMS:  now.Add(5 * time.Minute).UnixMilli(),
		CoverageComplete: true,
		Items:            []LogisticsItem{},
	}
	for _, entity := range entities {
		var state stateResponse
		if err := client.get(ctx, "/api/states/"+url.PathEscape(entity), &state); err != nil {
			return LogisticsView{}, err
		}
		updated, err := time.Parse(time.RFC3339Nano, state.LastUpdated)
		if err != nil || state.EntityID != entity || len(state.State) == 0 || len(state.State) > 128 || updated.After(now.Add(time.Minute)) || len(state.Attributes.FriendlyName) > 256 {
			return LogisticsView{}, ErrInvalidResponse
		}
		summary := strings.TrimSpace(state.Attributes.FriendlyName)
		if summary == "" {
			summary = "Selected home state"
		}
		view.Items = append(view.Items, LogisticsItem{
			EvidenceHandle: handle("home", client.connectionID+":"+entity),
			Kind:           "home_state",
			Summary:        summary,
			Status:         state.State,
			NeedsAttention: state.State == "unavailable" || state.State == "unknown",
		})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return LogisticsView{}, ErrInvalidResponse
	}
	return view, nil
}

func (client *Client) get(ctx context.Context, path string, output any) error {
	token, err := client.tokens.Token(ctx)
	if err != nil || len(token) < 8 || len(token) > 4096 || strings.ContainsAny(token, "\r\n") {
		return ErrCredentialExpired
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, client.baseURL+path, nil)
	if err != nil {
		return ErrInvalidInput
	}
	request.Header.Set("Authorization", "Bearer "+token)
	response, err := client.http.Do(request)
	if err != nil {
		return ErrUnavailable
	}
	defer response.Body.Close()
	switch response.StatusCode {
	case http.StatusOK:
	case http.StatusUnauthorized, http.StatusForbidden:
		return ErrCredentialExpired
	case http.StatusTooManyRequests:
		return ErrRateLimited
	default:
		return ErrUnavailable
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, 32*1024+1))
	if err != nil || len(data) > 32*1024 || json.Unmarshal(data, output) != nil {
		return ErrInvalidResponse
	}
	return nil
}

func handle(namespace, value string) string {
	digest := sha256.Sum256([]byte(namespace + "\x00" + value))
	return namespace + ":" + hex.EncodeToString(digest[:])
}

func validIdentifier(value string) bool {
	return strings.TrimSpace(value) != "" && len(value) <= 128
}
