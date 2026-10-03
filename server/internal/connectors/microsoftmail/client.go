package microsoftmail

import (
 "floe/server/internal/views"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

const (
	defaultBaseURL = "https://graph.microsoft.com/v1.0"
	maxItems       = 100
	maxResponse    = 512 * 1024
	observeScope   = "Mail.Read"
)

var (
	ErrInvalidInput      = errors.New("invalid Microsoft Mail input")
	ErrCredentialExpired = errors.New("Microsoft Mail credential expired")
	ErrPermissionDenied  = errors.New("Microsoft Mail permission denied")
	ErrRateLimited       = errors.New("Microsoft Mail rate limited")
	ErrUnavailable       = errors.New("Microsoft Mail unavailable")
	ErrInvalidResponse   = errors.New("invalid Microsoft Mail response")
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

type messageList struct {
	NextLink string    `json:"@odata.nextLink"`
	Value    []message `json:"value"`
}

type message struct {
	ID               string   `json:"id"`
	ConversationID   string   `json:"conversationId"`
	ReceivedDateTime string   `json:"receivedDateTime"`
	Subject          string   `json:"subject"`
	BodyPreview      string   `json:"bodyPreview"`
	Categories       []string `json:"categories"`
	From             struct {
		EmailAddress struct {
			Address string `json:"address"`
		} `json:"emailAddress"`
	} `json:"from"`
	ToRecipients []struct {
		EmailAddress struct {
			Address string `json:"address"`
		} `json:"emailAddress"`
	} `json:"toRecipients"`
}



func New(tokens TokenSource, connectionID string) (*Client, error) {
	return NewWithBaseURL(tokens, defaultBaseURL, connectionID)
}

func NewWithBaseURL(tokens TokenSource, baseURL, connectionID string) (*Client, error) {
	if tokens == nil || strings.TrimSpace(connectionID) == "" || len(connectionID) > 128 {
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
		tokens: tokens, baseURL: strings.TrimSuffix(endpoint.String(), "/"), connectionID: connectionID,
		http: &http.Client{Timeout: 10 * time.Second, Transport: &http.Transport{Proxy: nil}, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }},
	}, nil
}

func (client *Client) Communication(ctx context.Context, query string, cursor, limit int, now time.Time) (views.CommunicationView, error) {
	if len(query) > 512 || cursor < 0 || cursor > 10_000 || limit < 1 || limit > maxItems || strings.ContainsAny(query, "\r\n\x00") {
		return views.CommunicationView{}, ErrInvalidInput
	}
	values := url.Values{
		"$orderby": {"receivedDateTime desc"},
		"$select":  {"id,conversationId,receivedDateTime,from,toRecipients,subject,bodyPreview,categories"},
		"$skip":    {strconv.Itoa(cursor)},
		"$top":     {strconv.Itoa(limit)},
	}
	if trimmed := strings.TrimSpace(query); trimmed != "" {
		values.Del("$orderby")
		values.Set("$search", `"`+strings.ReplaceAll(trimmed, `"`, `\"`)+`"`)
	}
	var response messageList
	if err := client.get(ctx, "/me/mailFolders/inbox/messages?"+values.Encode(), &response); err != nil {
		return views.CommunicationView{}, err
	}
	if len(response.Value) > limit {
		return views.CommunicationView{}, ErrInvalidResponse
	}
	view := views.CommunicationView{SchemaVersion: 1, ViewID: "mail.communication", SourceHandle: handle("mail", client.connectionID+":"+now.UTC().Format("2006-01-02T15:04")+":"+query), ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(5 * time.Minute).UnixMilli(), CoverageComplete: strings.TrimSpace(response.NextLink) == "", Items: []views.CommunicationItem{}}
	if !view.CoverageComplete {
		next := cursor + len(response.Value)
		if next <= cursor || next > 10_000 {
			return views.CommunicationView{}, ErrInvalidResponse
		}
		view.NextCursor = &next
	}
	seen := map[string]bool{}
	for _, item := range response.Value {
		received, err := time.Parse(time.RFC3339Nano, item.ReceivedDateTime)
		if err != nil || !validProviderID(item.ID) || !validProviderID(item.ConversationID) || seen[item.ID] || received.UnixMilli() < 0 || received.After(now) || len(item.Subject) > 4096 || len(item.BodyPreview) > 4096 || len(item.Categories) > 128 {
			return views.CommunicationView{}, ErrInvalidResponse
		}
		seen[item.ID] = true
		to := make([]string, 0, len(item.ToRecipients))
		for _, recipient := range item.ToRecipients {
			if len(recipient.EmailAddress.Address) > 512 {
				return views.CommunicationView{}, ErrInvalidResponse
			}
			to = append(to, recipient.EmailAddress.Address)
		}
		if len(item.From.EmailAddress.Address) > 512 || len(strings.Join(to, ", ")) > 4096 {
			return views.CommunicationView{}, ErrInvalidResponse
		}
		labels := make([]string, 0, len(item.Categories))
		labelSeen := map[string]bool{}
		for _, label := range item.Categories {
			if strings.TrimSpace(label) == "" || len(label) > 128 || labelSeen[label] {
				return views.CommunicationView{}, ErrInvalidResponse
			}
			labelSeen[label] = true
			labels = append(labels, label)
		}
		view.Items = append(view.Items, views.CommunicationItem{EvidenceHandle: handle("mail", client.connectionID+":"+item.ID), ThreadHandle: handle("mail", client.connectionID+":"+item.ConversationID), ReceivedUnixMS: received.UnixMilli(), From: item.From.EmailAddress.Address, To: strings.Join(to, ", "), Subject: item.Subject, Snippet: truncate(item.BodyPreview, 1024), Labels: labels})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return views.CommunicationView{}, ErrInvalidResponse
	}
	return view, nil
}

func (client *Client) get(ctx context.Context, path string, output any) error {
	token, err := client.tokens.Token(ctx)
	if err != nil || len(token) < 8 || len(token) > 16_384 || strings.ContainsAny(token, "\r\n") {
		return ErrCredentialExpired
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, client.baseURL+path, nil)
	if err != nil {
		return ErrInvalidInput
	}
	request.Header.Set("Authorization", "Bearer "+token)
	if strings.Contains(path, "%24search=") {
		request.Header.Set("ConsistencyLevel", "eventual")
	}
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

func handle(namespace, value string) string {
	digest := sha256.Sum256([]byte(namespace + "\x00" + value))
	return namespace + ":" + hex.EncodeToString(digest[:16])
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

func validProviderID(value string) bool {
	return value != "" && len(value) <= 512 && !strings.ContainsAny(value, "\r\n\x00")
}
