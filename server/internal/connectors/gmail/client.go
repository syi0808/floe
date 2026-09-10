package gmail

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

const (
	DefaultBaseURL = "https://gmail.googleapis.com/gmail/v1"
	MaxPageItems   = 100
	MaxBodyBytes   = 32 * 1024
	maxEnvelope    = 1024 * 1024
)

var (
	ErrInvalidInput      = errors.New("invalid gmail request")
	ErrCredentialExpired = errors.New("gmail credential expired")
	ErrRateLimited       = errors.New("gmail rate limited")
	ErrCheckpointExpired = errors.New("gmail checkpoint expired")
	ErrUnavailable       = errors.New("gmail unavailable")
	ErrBodyApproval      = errors.New("gmail body read requires explicit approval")
	ErrInvalidResponse   = errors.New("invalid gmail response")
)

type TokenSource interface {
	Token(context.Context) (string, error)
}

type BodyReadAuthority interface {
	AuthorizeBodyRead(context.Context, string) error
}

type Client struct {
	baseURL string
	tokens  TokenSource
	http    *http.Client
}

type MessageRef struct {
	ID       string `json:"id"`
	ThreadID string `json:"thread_id"`
}

type Page struct {
	Messages   []MessageRef `json:"messages"`
	NextCursor string       `json:"next_cursor,omitempty"`
}

type Metadata struct {
	MessageRef
	HistoryID  string   `json:"history_id"`
	ReceivedMS int64    `json:"received_unix_ms"`
	Labels     []string `json:"labels"`
	From       string   `json:"from,omitempty"`
	To         string   `json:"to,omitempty"`
	Subject    string   `json:"subject,omitempty"`
	Date       string   `json:"date,omitempty"`
	Snippet    string   `json:"snippet,omitempty"`
}

type ChangePage struct {
	Added      []MessageRef `json:"added"`
	Deleted    []MessageRef `json:"deleted"`
	HistoryID  string       `json:"history_id"`
	NextCursor string       `json:"next_cursor,omitempty"`
}

func New(tokens TokenSource) (*Client, error) {
	return NewWithBaseURL(tokens, DefaultBaseURL)
}

func NewWithBaseURL(tokens TokenSource, baseURL string) (*Client, error) {
	endpoint, err := url.Parse(strings.TrimRight(baseURL, "/"))
	if err != nil || tokens == nil || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.Fragment != "" {
		return nil, ErrInvalidInput
	}
	ip := net.ParseIP(endpoint.Hostname())
	loopback := ip != nil && ip.IsLoopback()
	if endpoint.Hostname() == "" || (endpoint.Scheme != "https" && !(endpoint.Scheme == "http" && loopback)) {
		return nil, ErrInvalidInput
	}
	transport := &http.Transport{
		Proxy:               nil,
		DialContext:         (&net.Dialer{Timeout: 5 * time.Second, KeepAlive: 30 * time.Second}).DialContext,
		TLSClientConfig:     &tls.Config{MinVersion: tls.VersionTLS12},
		TLSHandshakeTimeout: 5 * time.Second,
		MaxIdleConns:        4,
		IdleConnTimeout:     30 * time.Second,
	}
	return &Client{baseURL: endpoint.String(), tokens: tokens, http: &http.Client{
		Transport:     transport,
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}}, nil
}

func (client *Client) Search(ctx context.Context, query, cursor string, limit int) (Page, error) {
	if strings.TrimSpace(query) == "" || len(query) > 512 || !validCursor(cursor) || limit < 1 || limit > MaxPageItems {
		return Page{}, ErrInvalidInput
	}
	parameters := url.Values{"q": {query}, "maxResults": {strconv.Itoa(limit)}}
	if cursor != "" {
		parameters.Set("pageToken", cursor)
	}
	var response listResponse
	if err := client.get(ctx, "/users/me/messages?"+parameters.Encode(), &response); err != nil {
		return Page{}, err
	}
	page := Page{NextCursor: response.NextPageToken}
	for _, message := range response.Messages {
		if !validID(message.ID) || !validID(message.ThreadID) {
			return Page{}, ErrInvalidResponse
		}
		page.Messages = append(page.Messages, MessageRef{ID: message.ID, ThreadID: message.ThreadID})
	}
	if len(page.Messages) > limit || !validCursor(page.NextCursor) {
		return Page{}, ErrInvalidResponse
	}
	return page, nil
}

func (client *Client) ReadMetadata(ctx context.Context, messageID string) (Metadata, error) {
	if !validID(messageID) {
		return Metadata{}, ErrInvalidInput
	}
	parameters := url.Values{"format": {"metadata"}}
	for _, header := range []string{"From", "To", "Subject", "Date"} {
		parameters.Add("metadataHeaders", header)
	}
	var response messageResponse
	if err := client.get(ctx, "/users/me/messages/"+url.PathEscape(messageID)+"?"+parameters.Encode(), &response); err != nil {
		return Metadata{}, err
	}
	return response.metadata()
}

func (client *Client) ReadBody(ctx context.Context, messageID string, authority BodyReadAuthority) (string, error) {
	if !validID(messageID) {
		return "", ErrInvalidInput
	}
	if authority == nil || authority.AuthorizeBodyRead(ctx, messageID) != nil {
		return "", ErrBodyApproval
	}
	var response messageResponse
	if err := client.get(ctx, "/users/me/messages/"+url.PathEscape(messageID)+"?format=full", &response); err != nil {
		return "", err
	}
	body, found, err := textBody(response.Payload)
	if err != nil || !found || len(body) > MaxBodyBytes {
		return "", ErrInvalidResponse
	}
	return body, nil
}

func (client *Client) Changes(ctx context.Context, startHistoryID, cursor string, limit int) (ChangePage, error) {
	if !validID(startHistoryID) || !validCursor(cursor) || limit < 1 || limit > MaxPageItems {
		return ChangePage{}, ErrInvalidInput
	}
	parameters := url.Values{"startHistoryId": {startHistoryID}, "maxResults": {strconv.Itoa(limit)}}
	parameters.Add("historyTypes", "messageAdded")
	parameters.Add("historyTypes", "messageDeleted")
	if cursor != "" {
		parameters.Set("pageToken", cursor)
	}
	var response historyResponse
	if err := client.get(ctx, "/users/me/history?"+parameters.Encode(), &response); err != nil {
		return ChangePage{}, err
	}
	page := ChangePage{HistoryID: response.HistoryID, NextCursor: response.NextPageToken}
	if !validID(page.HistoryID) || !validCursor(page.NextCursor) {
		return ChangePage{}, ErrInvalidResponse
	}
	seenAdded, seenDeleted := map[string]bool{}, map[string]bool{}
	for _, history := range response.History {
		for _, added := range history.MessagesAdded {
			if err := appendRef(&page.Added, added.Message, seenAdded); err != nil {
				return ChangePage{}, err
			}
		}
		for _, deleted := range history.MessagesDeleted {
			if err := appendRef(&page.Deleted, deleted.Message, seenDeleted); err != nil {
				return ChangePage{}, err
			}
		}
	}
	if len(page.Added)+len(page.Deleted) > limit*2 {
		return ChangePage{}, ErrInvalidResponse
	}
	return page, nil
}

func (client *Client) get(ctx context.Context, path string, output any) error {
	token, err := client.tokens.Token(ctx)
	if err != nil || strings.TrimSpace(token) == "" || len(token) > 8192 || strings.ContainsAny(token, "\r\n") {
		return ErrCredentialExpired
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, client.baseURL+path, nil)
	if err != nil {
		return ErrInvalidInput
	}
	request.Header.Set("Authorization", "Bearer "+token)
	response, err := client.http.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return ErrUnavailable
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		switch response.StatusCode {
		case http.StatusUnauthorized, http.StatusForbidden:
			return ErrCredentialExpired
		case http.StatusTooManyRequests:
			return ErrRateLimited
		case http.StatusNotFound:
			if strings.Contains(path, "/history?") {
				return ErrCheckpointExpired
			}
			return ErrInvalidInput
		default:
			return ErrUnavailable
		}
	}
	body, err := io.ReadAll(io.LimitReader(response.Body, maxEnvelope+1))
	if err != nil || len(body) > maxEnvelope {
		return ErrInvalidResponse
	}
	decoder := json.NewDecoder(bytes.NewReader(body))
	if decoder.Decode(output) != nil || decoder.Decode(new(any)) != io.EOF {
		return ErrInvalidResponse
	}
	return nil
}

type listResponse struct {
	Messages      []messageResponse `json:"messages"`
	NextPageToken string            `json:"nextPageToken"`
}
type header struct {
	Name  string `json:"name"`
	Value string `json:"value"`
}
type payload struct {
	MimeType string   `json:"mimeType"`
	Headers  []header `json:"headers"`
	Body     struct {
		Data string `json:"data"`
	} `json:"body"`
	Parts []payload `json:"parts"`
}
type messageResponse struct {
	ID           string   `json:"id"`
	ThreadID     string   `json:"threadId"`
	HistoryID    string   `json:"historyId"`
	InternalDate string   `json:"internalDate"`
	LabelIDs     []string `json:"labelIds"`
	Snippet      string   `json:"snippet"`
	Payload      payload  `json:"payload"`
}
type historyMessage struct {
	Message messageResponse `json:"message"`
}
type historyEntry struct {
	MessagesAdded   []historyMessage `json:"messagesAdded"`
	MessagesDeleted []historyMessage `json:"messagesDeleted"`
}
type historyResponse struct {
	History       []historyEntry `json:"history"`
	NextPageToken string         `json:"nextPageToken"`
	HistoryID     string         `json:"historyId"`
}

func (message messageResponse) metadata() (Metadata, error) {
	if !validID(message.ID) || !validID(message.ThreadID) || !validID(message.HistoryID) || len(message.Snippet) > 1024 || len(message.LabelIDs) > 128 {
		return Metadata{}, ErrInvalidResponse
	}
	received, err := strconv.ParseInt(message.InternalDate, 10, 64)
	if err != nil || received < 0 {
		return Metadata{}, ErrInvalidResponse
	}
	result := Metadata{MessageRef: MessageRef{ID: message.ID, ThreadID: message.ThreadID}, HistoryID: message.HistoryID, ReceivedMS: received, Labels: message.LabelIDs, Snippet: message.Snippet}
	for _, item := range message.Payload.Headers {
		if len(item.Value) > 4096 {
			return Metadata{}, ErrInvalidResponse
		}
		switch strings.ToLower(item.Name) {
		case "from":
			result.From = item.Value
		case "to":
			result.To = item.Value
		case "subject":
			result.Subject = item.Value
		case "date":
			result.Date = item.Value
		}
	}
	return result, nil
}

func textBody(part payload) (string, bool, error) {
	if part.MimeType == "text/plain" && part.Body.Data != "" {
		decoded, err := base64.RawURLEncoding.DecodeString(part.Body.Data)
		if err != nil {
			return "", false, err
		}
		return string(decoded), true, nil
	}
	for _, child := range part.Parts {
		if body, found, err := textBody(child); err != nil || found {
			return body, found, err
		}
	}
	return "", false, nil
}

func appendRef(target *[]MessageRef, message messageResponse, seen map[string]bool) error {
	if !validID(message.ID) || !validID(message.ThreadID) {
		return ErrInvalidResponse
	}
	if !seen[message.ID] {
		*target = append(*target, MessageRef{ID: message.ID, ThreadID: message.ThreadID})
		seen[message.ID] = true
	}
	return nil
}

func validID(value string) bool {
	return value != "" && len(value) <= 256 && !strings.ContainsAny(value, "\r\n/\\")
}
func validCursor(value string) bool { return len(value) <= 2048 && !strings.ContainsAny(value, "\r\n") }
