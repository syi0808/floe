package microsoftteams

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"html"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"

	"floe/server/internal/connectors/common"
)

const (
	defaultBaseURL = "https://graph.microsoft.com/v1.0"
	maxMessages    = 50
	maxResponse    = 256 * 1024
)

var (
	selectionPattern = regexp.MustCompile(`^[A-Za-z0-9@._:-]{1,256}$`)
	tagPattern       = regexp.MustCompile(`(?s)<[^>]*>`)
	hiddenPattern    = regexp.MustCompile(`(?is)<(?:script|style)[^>]*>.*?</(?:script|style)>`)
	breakPattern     = regexp.MustCompile(`(?i)<(?:br|/p|/div)\s*/?>`)

	ErrInvalidInput      = errors.New("invalid Microsoft Teams input")
	ErrCredentialExpired = errors.New("Microsoft Teams credential expired")
	ErrPermissionDenied  = errors.New("Microsoft Teams permission denied")
	ErrRateLimited       = errors.New("Microsoft Teams rate limited")
	ErrUnavailable       = errors.New("Microsoft Teams unavailable")
	ErrInvalidResponse   = errors.New("invalid Microsoft Teams response")
)

type TokenSource interface {
	Token(context.Context) (string, error)
}

type Client struct {
	tokens  TokenSource
	baseURL string
	http    *http.Client
}

type messageResponse struct {
	Next  string    `json:"@odata.nextLink"`
	Value []message `json:"value"`
}

type message struct {
	ID              string  `json:"id"`
	Created         string  `json:"createdDateTime"`
	LastModified    string  `json:"lastModifiedDateTime"`
	MessageType     string  `json:"messageType"`
	DeletedDateTime *string `json:"deletedDateTime"`
	Body            struct {
		ContentType string `json:"contentType"`
		Content     string `json:"content"`
	} `json:"body"`
}

func New(tokens TokenSource) (*Client, error) {
	return NewWithBaseURL(tokens, defaultBaseURL)
}

func NewWithBaseURL(tokens TokenSource, baseURL string) (*Client, error) {
	if tokens == nil {
		return nil, ErrInvalidInput
	}
	endpoint, err := url.Parse(baseURL)
	if err != nil || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.Fragment != "" {
		return nil, ErrInvalidInput
	}
	loopback := endpoint.Scheme == "http" && (endpoint.Hostname() == "127.0.0.1" || endpoint.Hostname() == "localhost")
	path := strings.TrimSuffix(endpoint.Path, "/")
	if endpoint.Scheme != "https" && !loopback || path != "/v1.0" && !(loopback && path == "") {
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

func (client *Client) WorkContext(ctx context.Context, team, channel string, now time.Time) (common.WorkContextView, error) {
	if !selectionPattern.MatchString(team) || !selectionPattern.MatchString(channel) {
		return common.WorkContextView{}, ErrInvalidInput
	}
	path := "/teams/" + url.PathEscape(team) + "/channels/" + url.PathEscape(channel) + "/messages?%24top=50"
	var response messageResponse
	if err := client.get(ctx, path, &response); err != nil {
		return common.WorkContextView{}, err
	}
	if len(response.Value) > maxMessages {
		return common.WorkContextView{}, ErrInvalidResponse
	}
	view := common.WorkContextView{
		SchemaVersion:    1,
		ViewID:           "work.context",
		SourceHandle:     handle("teams", team+":"+channel+":"+now.UTC().Format("2006-01-02T15:04")),
		ObservedAtUnixMS: now.UnixMilli(),
		ExpiresAtUnixMS:  now.Add(5 * time.Minute).UnixMilli(),
		CoverageComplete: strings.TrimSpace(response.Next) == "",
		ScopeHandle:      handle("teams-channel", team+":"+channel),
		Items:            []common.WorkItem{},
	}
	seen := map[string]bool{}
	for _, item := range response.Value {
		if item.MessageType != "message" || item.DeletedDateTime != nil {
			continue
		}
		if !validOpaque(item.ID, 512) || seen[item.ID] || len(item.Body.Content) > 65_536 {
			return common.WorkContextView{}, ErrInvalidResponse
		}
		seen[item.ID] = true
		observed, err := parseTime(item.LastModified)
		if item.LastModified == "" {
			observed, err = parseTime(item.Created)
		}
		text, textErr := messageText(item.Body.ContentType, item.Body.Content)
		if err != nil || textErr != nil || observed > now.Add(time.Minute).UnixMilli() {
			return common.WorkContextView{}, ErrInvalidResponse
		}
		if text == "" {
			continue
		}
		excerpt := truncate(text, 2048)
		title := truncate(strings.TrimSpace(strings.SplitN(excerpt, "\n", 2)[0]), 512)
		status := "message"
		view.Items = append(view.Items, common.WorkItem{
			EvidenceHandle:   handle("teams-message", team+":"+channel+":"+item.ID),
			Kind:             "communication",
			Title:            title,
			Excerpt:          &excerpt,
			Status:           &status,
			ObservedAtUnixMS: observed,
		})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return common.WorkContextView{}, ErrInvalidResponse
	}
	return view, nil
}

func (client *Client) get(ctx context.Context, path string, output any) error {
	token, err := client.tokens.Token(ctx)
	if err != nil || !validOpaque(token, 4096) || len(token) < 8 {
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
	case http.StatusUnauthorized:
		return ErrCredentialExpired
	case http.StatusForbidden:
		return ErrPermissionDenied
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

func messageText(contentType, content string) (string, error) {
	switch strings.ToLower(strings.TrimSpace(contentType)) {
	case "text":
		return strings.TrimSpace(content), nil
	case "html":
		content = hiddenPattern.ReplaceAllString(content, " ")
		content = breakPattern.ReplaceAllString(content, "\n")
		content = tagPattern.ReplaceAllString(content, " ")
		return strings.TrimSpace(html.UnescapeString(content)), nil
	default:
		return "", ErrInvalidResponse
	}
}

func parseTime(value string) (int64, error) {
	parsed, err := time.Parse(time.RFC3339Nano, value)
	if err != nil {
		return 0, ErrInvalidResponse
	}
	return parsed.UnixMilli(), nil
}

func validOpaque(value string, maximum int) bool {
	return strings.TrimSpace(value) != "" && len(value) <= maximum && !strings.ContainsAny(value, "\x00\r\n")
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
