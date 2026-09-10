package googlecalendar

import (
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
	defaultBaseURL = "https://www.googleapis.com/calendar/v3"
	maxItems       = 128
	maxResponse    = 512 * 1024
	observeScope   = "https://www.googleapis.com/auth/calendar.readonly"
)

var (
	ErrInvalidInput      = errors.New("invalid Google Calendar input")
	ErrCredentialExpired = errors.New("Google Calendar credential expired")
	ErrPermissionDenied  = errors.New("Google Calendar permission denied")
	ErrRateLimited       = errors.New("Google Calendar rate limited")
	ErrUnavailable       = errors.New("Google Calendar unavailable")
	ErrInvalidResponse   = errors.New("invalid Google Calendar response")
)

type TokenSource interface {
	Token(context.Context) (string, error)
}

type Client struct {
	tokens       TokenSource
	baseURL      string
	calendarID   string
	connectionID string
	http         *http.Client
}

type eventList struct {
	NextPageToken string          `json:"nextPageToken"`
	Items         []calendarEvent `json:"items"`
}

type calendarEvent struct {
	ID      string        `json:"id"`
	Status  string        `json:"status"`
	Summary string        `json:"summary"`
	Start   eventDateTime `json:"start"`
	End     eventDateTime `json:"end"`
}

type eventDateTime struct {
	DateTime string `json:"dateTime"`
	Date     string `json:"date"`
}

type CalendarItem struct {
	EvidenceHandle string `json:"evidence_handle"`
	UntrustedTitle string `json:"untrusted_title"`
	StartsAtUnixMS int64  `json:"starts_at_unix_ms"`
	EndsAtUnixMS   int64  `json:"ends_at_unix_ms"`
	AllDay         bool   `json:"all_day"`
}

type CalendarView struct {
	SchemaVersion    int            `json:"schema_version"`
	ViewID           string         `json:"view_id"`
	SourceHandle     string         `json:"source_handle"`
	ObservedAtUnixMS int64          `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64          `json:"expires_at_unix_ms"`
	RangeStartUnixMS int64          `json:"range_start_unix_ms"`
	RangeEndUnixMS   int64          `json:"range_end_unix_ms"`
	CoverageComplete bool           `json:"coverage_complete"`
	NextCursor       *string        `json:"next_cursor,omitempty"`
	Items            []CalendarItem `json:"items"`
}

func New(tokens TokenSource, calendarID, connectionID string) (*Client, error) {
	return NewWithBaseURL(tokens, defaultBaseURL, calendarID, connectionID)
}

func NewWithBaseURL(tokens TokenSource, baseURL, calendarID, connectionID string) (*Client, error) {
	if tokens == nil || !validOpaque(calendarID, 512) || !validOpaque(connectionID, 128) {
		return nil, ErrInvalidInput
	}
	endpoint, err := url.Parse(baseURL)
	if err != nil || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.Fragment != "" {
		return nil, ErrInvalidInput
	}
	loopback := endpoint.Scheme == "http" && (endpoint.Hostname() == "127.0.0.1" || endpoint.Hostname() == "localhost")
	path := strings.TrimSuffix(endpoint.Path, "/")
	if endpoint.Scheme != "https" && !loopback || path != "/calendar/v3" && !(loopback && path == "") {
		return nil, ErrInvalidInput
	}
	return &Client{tokens: tokens, baseURL: strings.TrimSuffix(endpoint.String(), "/"), calendarID: calendarID, connectionID: connectionID, http: &http.Client{Timeout: 10 * time.Second, Transport: &http.Transport{Proxy: nil}, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}}, nil
}

func (client *Client) Calendar(ctx context.Context, rangeStart, rangeEnd time.Time, cursor string, limit int, now time.Time) (CalendarView, error) {
	if rangeStart.IsZero() || rangeEnd.IsZero() || !rangeStart.Before(rangeEnd) || rangeEnd.Sub(rangeStart) > 32*24*time.Hour || rangeStart.UnixMilli() < 0 || len(cursor) > 2048 || strings.ContainsAny(cursor, "\r\n\x00") || limit < 1 || limit > maxItems {
		return CalendarView{}, ErrInvalidInput
	}
	values := url.Values{"singleEvents": {"true"}, "orderBy": {"startTime"}, "timeZone": {"UTC"}, "timeMin": {rangeStart.UTC().Format(time.RFC3339Nano)}, "timeMax": {rangeEnd.UTC().Format(time.RFC3339Nano)}, "maxResults": {strconv.Itoa(limit)}, "fields": {"items(id,status,summary,start,end),nextPageToken"}}
	if cursor != "" {
		values.Set("pageToken", cursor)
	}
	var response eventList
	path := "/calendars/" + url.PathEscape(client.calendarID) + "/events?" + values.Encode()
	if err := client.get(ctx, path, &response); err != nil {
		return CalendarView{}, err
	}
	if len(response.Items) > limit || !validCursor(response.NextPageToken) {
		return CalendarView{}, ErrInvalidResponse
	}
	view := CalendarView{SchemaVersion: 1, ViewID: "calendar.timeline", SourceHandle: handle("calendar.timeline", client.connectionID+":"+client.calendarID), ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(5 * time.Minute).UnixMilli(), RangeStartUnixMS: rangeStart.UnixMilli(), RangeEndUnixMS: rangeEnd.UnixMilli(), CoverageComplete: response.NextPageToken == "", Items: []CalendarItem{}}
	if response.NextPageToken != "" {
		view.NextCursor = &response.NextPageToken
	}
	seen := map[string]bool{}
	for _, event := range response.Items {
		if event.Status == "cancelled" {
			continue
		}
		if event.Status != "confirmed" && event.Status != "tentative" || !validOpaque(event.ID, 512) || seen[event.ID] || len(event.Summary) > 4096 {
			return CalendarView{}, ErrInvalidResponse
		}
		starts, startAllDay, err := parseEventTime(event.Start)
		if err != nil {
			return CalendarView{}, ErrInvalidResponse
		}
		ends, endAllDay, err := parseEventTime(event.End)
		if err != nil || startAllDay != endAllDay || starts.UnixMilli() < 0 || !starts.Before(ends) || !starts.Before(rangeEnd) || !ends.After(rangeStart) {
			return CalendarView{}, ErrInvalidResponse
		}
		seen[event.ID] = true
		view.Items = append(view.Items, CalendarItem{EvidenceHandle: handle("calendar.event", client.connectionID+":"+event.ID), UntrustedTitle: truncate(event.Summary, 1024), StartsAtUnixMS: starts.UnixMilli(), EndsAtUnixMS: ends.UnixMilli(), AllDay: startAllDay})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return CalendarView{}, ErrInvalidResponse
	}
	return view, nil
}

func (client *Client) get(ctx context.Context, path string, output any) error {
	token, err := client.tokens.Token(ctx)
	if err != nil || !validOpaque(token, 16_384) {
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

func parseEventTime(value eventDateTime) (time.Time, bool, error) {
	if value.DateTime != "" && value.Date != "" || value.DateTime == "" && value.Date == "" {
		return time.Time{}, false, ErrInvalidResponse
	}
	if value.DateTime != "" {
		parsed, err := time.Parse(time.RFC3339Nano, value.DateTime)
		return parsed, false, err
	}
	parsed, err := time.Parse("2006-01-02", value.Date)
	return parsed, true, err
}

func validOpaque(value string, maximum int) bool {
	return strings.TrimSpace(value) != "" && len(value) <= maximum && !strings.ContainsAny(value, "\r\n\x00")
}

func validCursor(value string) bool {
	return value == "" || validOpaque(value, 2048)
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
