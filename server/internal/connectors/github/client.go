package github

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

const (
	defaultBaseURL = "https://api.github.com"
	maxResponse    = 512 * 1024
	maxIssues      = 50
)

var identifier = regexp.MustCompile(`^[A-Za-z0-9_.-]{1,100}$`)

var (
	ErrInvalidInput      = errors.New("invalid github input")
	ErrCredentialExpired = errors.New("github credential expired")
	ErrRateLimited       = errors.New("github rate limited")
	ErrUnavailable       = errors.New("github unavailable")
	ErrInvalidResponse   = errors.New("invalid github response")
)

type TokenSource interface {
	Token(context.Context) (string, error)
}

type Client struct {
	tokens  TokenSource
	baseURL string
	http    *http.Client
}

type issue struct {
	Number      int64  `json:"number"`
	Title       string `json:"title"`
	Body        string `json:"body"`
	State       string `json:"state"`
	UpdatedAt   string `json:"updated_at"`
	PullRequest any    `json:"pull_request"`
	Labels      []struct {
		Name string `json:"name"`
	} `json:"labels"`
}

type WorkItem struct {
	EvidenceHandle   string  `json:"evidence_handle"`
	Kind             string  `json:"kind"`
	Title            string  `json:"title"`
	Excerpt          *string `json:"excerpt,omitempty"`
	Status           *string `json:"status,omitempty"`
	Blocker          *string `json:"blocker,omitempty"`
	NextAction       *string `json:"next_action,omitempty"`
	ObservedAtUnixMS int64   `json:"observed_at_unix_ms"`
}

type WorkContextView struct {
	SchemaVersion    int        `json:"schema_version"`
	ViewID           string     `json:"view_id"`
	SourceHandle     string     `json:"source_handle"`
	ObservedAtUnixMS int64      `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64      `json:"expires_at_unix_ms"`
	CoverageComplete bool       `json:"coverage_complete"`
	ScopeHandle      string     `json:"scope_handle"`
	Items            []WorkItem `json:"items"`
}

func New(tokens TokenSource) (*Client, error) {
	return NewWithBaseURL(tokens, defaultBaseURL)
}

func NewWithBaseURL(tokens TokenSource, baseURL string) (*Client, error) {
	if tokens == nil {
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
		tokens:  tokens,
		baseURL: strings.TrimSuffix(endpoint.String(), "/"),
		http: &http.Client{
			Timeout:   10 * time.Second,
			Transport: &http.Transport{Proxy: nil},
			CheckRedirect: func(*http.Request, []*http.Request) error {
				return http.ErrUseLastResponse
			},
		},
	}, nil
}

func (client *Client) WorkContext(ctx context.Context, owner, repository string, now time.Time) (WorkContextView, error) {
	if !identifier.MatchString(owner) || !identifier.MatchString(repository) {
		return WorkContextView{}, ErrInvalidInput
	}
	var issues []issue
	path := fmt.Sprintf("/repos/%s/%s/issues?state=open&per_page=%d", url.PathEscape(owner), url.PathEscape(repository), maxIssues)
	if err := client.get(ctx, path, &issues); err != nil {
		return WorkContextView{}, err
	}
	if len(issues) > maxIssues {
		return WorkContextView{}, ErrInvalidResponse
	}
	view := WorkContextView{
		SchemaVersion:    1,
		ViewID:           "work.context",
		SourceHandle:     handle("github", owner+"/"+repository+":"+now.UTC().Format("2006-01-02T15:04")),
		ObservedAtUnixMS: now.UnixMilli(),
		ExpiresAtUnixMS:  now.Add(5 * time.Minute).UnixMilli(),
		CoverageComplete: len(issues) < maxIssues,
		ScopeHandle:      handle("workspace", owner+"/"+repository),
		Items:            []WorkItem{},
	}
	for _, item := range issues {
		if item.PullRequest != nil {
			continue
		}
		updated, err := time.Parse(time.RFC3339, item.UpdatedAt)
		if err != nil || item.Number < 1 || item.State != "open" || item.Title == "" || len(item.Title) > 512 || updated.After(now.Add(time.Minute)) {
			return WorkContextView{}, ErrInvalidResponse
		}
		excerpt := truncate(strings.TrimSpace(item.Body), 2048)
		var excerptPointer *string
		if excerpt != "" {
			excerptPointer = &excerpt
		}
		status := "open"
		var blocker *string
		for _, label := range item.Labels {
			if len(label.Name) > 128 {
				return WorkContextView{}, ErrInvalidResponse
			}
			if strings.EqualFold(label.Name, "blocked") || strings.EqualFold(label.Name, "status: blocked") {
				value := "Issue is labeled blocked"
				blocker = &value
			}
		}
		view.Items = append(view.Items, WorkItem{
			EvidenceHandle:   handle("github", fmt.Sprintf("%s/%s#%d", owner, repository, item.Number)),
			Kind:             "project",
			Title:            item.Title,
			Excerpt:          excerptPointer,
			Status:           &status,
			Blocker:          blocker,
			ObservedAtUnixMS: updated.UnixMilli(),
		})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return WorkContextView{}, ErrInvalidResponse
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
	request.Header.Set("Accept", "application/vnd.github+json")
	request.Header.Set("X-GitHub-Api-Version", "2022-11-28")
	response, err := client.http.Do(request)
	if err != nil {
		return ErrUnavailable
	}
	defer response.Body.Close()
	switch response.StatusCode {
	case http.StatusOK:
	case http.StatusUnauthorized, http.StatusForbidden:
		if response.StatusCode == http.StatusForbidden && response.Header.Get("X-RateLimit-Remaining") == "0" {
			return ErrRateLimited
		}
		return ErrCredentialExpired
	case http.StatusTooManyRequests:
		return ErrRateLimited
	default:
		return ErrUnavailable
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, maxResponse+1))
	if err != nil || len(data) > maxResponse || json.Unmarshal(data, output) != nil {
		return ErrInvalidResponse
	}
	return nil
}

func handle(namespace, value string) string {
	digest := sha256.Sum256([]byte(namespace + "\x00" + value))
	return namespace + ":" + hex.EncodeToString(digest[:])
}

func truncate(value string, maximum int) string {
	if len(value) <= maximum {
		return value
	}
	for maximum > 0 && maximum < len(value) && value[maximum]&0xc0 == 0x80 {
		maximum--
	}
	return strings.TrimSpace(value[:maximum])
}
