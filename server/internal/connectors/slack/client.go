package slack

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
	"strconv"
	"strings"
	"time"

	"floe/server/internal/connectors/common"
)

const (
	defaultBaseURL = "https://slack.com/api"
	maxMessages    = 50
	maxResponse    = 256 * 1024
)

var (
	channelPattern = regexp.MustCompile(`^[CG][A-Z0-9]{8,20}$`)
	threadPattern  = regexp.MustCompile(`^[0-9]{10,16}\.[0-9]{6}$`)

	ErrInvalidInput      = errors.New("invalid slack input")
	ErrCredentialExpired = errors.New("slack credential expired")
	ErrPermissionDenied  = errors.New("slack permission denied")
	ErrRateLimited       = errors.New("slack rate limited")
	ErrUnavailable       = errors.New("slack unavailable")
	ErrInvalidResponse   = errors.New("invalid slack response")
)

type TokenSource interface {
	Token(context.Context) (string, error)
}

type Client struct {
	tokens  TokenSource
	baseURL string
	http    *http.Client
}

type historyResponse struct {
	OK       bool      `json:"ok"`
	Error    string    `json:"error"`
	Messages []message `json:"messages"`
	Metadata struct {
		NextCursor string `json:"next_cursor"`
	} `json:"response_metadata"`
}

type message struct {
	Type     string `json:"type"`
	Subtype  string `json:"subtype"`
	Text     string `json:"text"`
	TS       string `json:"ts"`
	ThreadTS string `json:"thread_ts"`
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
	if endpoint.Scheme != "https" && !loopback || path != "/api" && !(loopback && path == "") {
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

func (client *Client) WorkContext(ctx context.Context, channel, thread string, now time.Time) (common.WorkContextView, error) {
	if !channelPattern.MatchString(channel) || thread != "" && !threadPattern.MatchString(thread) {
		return common.WorkContextView{}, ErrInvalidInput
	}
	values := url.Values{"channel": {channel}, "limit": {strconv.Itoa(maxMessages)}}
	method := "conversations.history"
	if thread != "" {
		method = "conversations.replies"
		values.Set("ts", thread)
	}
	var response historyResponse
	if err := client.get(ctx, "/"+method+"?"+values.Encode(), &response); err != nil {
		return common.WorkContextView{}, err
	}
	if !response.OK {
		return common.WorkContextView{}, providerError(response.Error)
	}
	if len(response.Messages) > maxMessages {
		return common.WorkContextView{}, ErrInvalidResponse
	}
	view := common.WorkContextView{
		SchemaVersion:    1,
		ViewID:           "work.context",
		SourceHandle:     handle("slack", channel+":"+thread+":"+now.UTC().Format("2006-01-02T15:04")),
		ObservedAtUnixMS: now.UnixMilli(),
		ExpiresAtUnixMS:  now.Add(5 * time.Minute).UnixMilli(),
		CoverageComplete: strings.TrimSpace(response.Metadata.NextCursor) == "",
		ScopeHandle:      handle("channel", channel+":"+thread),
		Items:            []common.WorkItem{},
	}
	seen := map[string]bool{}
	for _, item := range response.Messages {
		if item.Type != "message" || item.Subtype != "" {
			continue
		}
		observed, err := slackTimestamp(item.TS)
		text := strings.TrimSpace(item.Text)
		if err != nil || observed > now.Add(time.Minute).UnixMilli() || text == "" || seen[item.TS] {
			return common.WorkContextView{}, ErrInvalidResponse
		}
		seen[item.TS] = true
		excerpt := truncate(text, 2048)
		title := truncate(strings.TrimSpace(strings.SplitN(excerpt, "\n", 2)[0]), 512)
		status := "message"
		view.Items = append(view.Items, common.WorkItem{
			EvidenceHandle:   handle("slack", channel+":"+item.TS),
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
	if response.StatusCode == http.StatusTooManyRequests {
		return ErrRateLimited
	}
	if response.StatusCode == http.StatusUnauthorized || response.StatusCode == http.StatusForbidden {
		return ErrCredentialExpired
	}
	if response.StatusCode != http.StatusOK {
		return ErrUnavailable
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, maxResponse+1))
	if err != nil || len(data) > maxResponse || json.Unmarshal(data, output) != nil {
		return ErrInvalidResponse
	}
	return nil
}

func providerError(value string) error {
	switch value {
	case "invalid_auth", "account_inactive", "token_revoked":
		return ErrCredentialExpired
	case "missing_scope", "not_in_channel", "channel_not_found":
		return ErrPermissionDenied
	case "ratelimited":
		return ErrRateLimited
	default:
		return ErrUnavailable
	}
}

func slackTimestamp(value string) (int64, error) {
	parts := strings.Split(value, ".")
	if len(parts) != 2 || len(parts[1]) != 6 {
		return 0, ErrInvalidResponse
	}
	seconds, secondsErr := strconv.ParseInt(parts[0], 10, 64)
	micros, microsErr := strconv.ParseInt(parts[1], 10, 64)
	if secondsErr != nil || microsErr != nil || seconds < 0 || seconds > 9_000_000_000_000_000 {
		return 0, ErrInvalidResponse
	}
	return seconds*1000 + micros/1000, nil
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
