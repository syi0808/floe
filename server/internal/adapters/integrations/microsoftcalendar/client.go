package microsoftcalendar

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"floe/server/internal/views"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

const (
	defaultBaseURL = "https://graph.microsoft.com/v1.0"
	maxItems       = 128
	maxResponse    = 512 * 1024
	observeScope   = "Calendars.Read"
)

var (
	ErrInvalidInput      = errors.New("invalid Microsoft Calendar input")
	ErrCredentialExpired = errors.New("Microsoft Calendar credential expired")
	ErrPermissionDenied  = errors.New("Microsoft Calendar permission denied")
	ErrRateLimited       = errors.New("Microsoft Calendar rate limited")
	ErrUnavailable       = errors.New("Microsoft Calendar unavailable")
	ErrInvalidResponse   = errors.New("invalid Microsoft Calendar response")
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
	NextLink string          `json:"@odata.nextLink"`
	Value    []calendarEvent `json:"value"`
}

type calendarEvent struct {
	ID          string        `json:"id"`
	Subject     string        `json:"subject"`
	Start       eventDateTime `json:"start"`
	End         eventDateTime `json:"end"`
	IsAllDay    bool          `json:"isAllDay"`
	IsCancelled bool          `json:"isCancelled"`
}

type eventDateTime struct {
	DateTime string `json:"dateTime"`
	TimeZone string `json:"timeZone"`
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
	if endpoint.Scheme != "https" && !loopback || path != "/v1.0" && !(loopback && path == "") {
		return nil, ErrInvalidInput
	}
	return &Client{tokens: tokens, baseURL: strings.TrimSuffix(endpoint.String(), "/"), calendarID: calendarID, connectionID: connectionID, http: &http.Client{Timeout: 10 * time.Second, Transport: &http.Transport{Proxy: nil}, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}}, nil
}

func (client *Client) Calendar(ctx context.Context, rangeStart, rangeEnd time.Time, cursor string, limit int, now time.Time) (views.CalendarView, error) {
	if rangeStart.IsZero() || rangeEnd.IsZero() || !rangeStart.Before(rangeEnd) || rangeEnd.Sub(rangeStart) > 32*24*time.Hour || rangeStart.UnixMilli() < 0 || limit < 1 || limit > maxItems {
		return views.CalendarView{}, ErrInvalidInput
	}
	values := url.Values{"startDateTime": {rangeStart.UTC().Format(time.RFC3339Nano)}, "endDateTime": {rangeEnd.UTC().Format(time.RFC3339Nano)}, "$top": {strconv.Itoa(limit)}, "$select": {"id,subject,start,end,isAllDay,isCancelled"}}
	if cursor != "" {
		name, value, ok := decodeCursor(cursor)
		if !ok {
			return views.CalendarView{}, ErrInvalidInput
		}
		values.Set(name, value)
	}
	path := "/me/calendars/" + url.PathEscape(client.calendarID) + "/calendarView"
	var response eventList
	if err := client.get(ctx, path+"?"+values.Encode(), &response); err != nil {
		return views.CalendarView{}, err
	}
	if len(response.Value) > limit {
		return views.CalendarView{}, ErrInvalidResponse
	}
	nextCursor, err := client.nextCursor(response.NextLink)
	if err != nil {
		return views.CalendarView{}, err
	}
	view := views.CalendarView{SchemaVersion: 1, ViewID: "calendar.timeline", SourceHandle: handle("calendar.timeline", client.connectionID+":"+client.calendarID), ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(5 * time.Minute).UnixMilli(), RangeStartUnixMS: rangeStart.UnixMilli(), RangeEndUnixMS: rangeEnd.UnixMilli(), CoverageComplete: nextCursor == "", Items: []views.CalendarItem{}}
	if nextCursor != "" {
		view.NextCursor = &nextCursor
	}
	seen := map[string]bool{}
	for _, event := range response.Value {
		if event.IsCancelled {
			continue
		}
		if !validOpaque(event.ID, 512) || seen[event.ID] || len(event.Subject) > 4096 {
			return views.CalendarView{}, ErrInvalidResponse
		}
		starts, err := parseEventTime(event.Start)
		if err != nil {
			return views.CalendarView{}, ErrInvalidResponse
		}
		ends, err := parseEventTime(event.End)
		if err != nil || starts.UnixMilli() < 0 || !starts.Before(ends) || !starts.Before(rangeEnd) || !ends.After(rangeStart) {
			return views.CalendarView{}, ErrInvalidResponse
		}
		seen[event.ID] = true
		view.Items = append(view.Items, views.CalendarItem{EvidenceHandle: handle("calendar.event", client.connectionID+":"+client.calendarID+":"+event.ID), UntrustedTitle: truncate(event.Subject, 1024), StartsAtUnixMS: starts.UnixMilli(), EndsAtUnixMS: ends.UnixMilli(), AllDay: event.IsAllDay})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return views.CalendarView{}, ErrInvalidResponse
	}
	return view, nil
}

func (client *Client) get(ctx context.Context, path string, output any) error {
	token, err := client.tokens.Token(ctx)
	if err != nil {
		if contextErr := contextReadError(ctx, err); contextErr != nil {
			return contextErr
		}
		return ErrCredentialExpired
	}
	if contextErr := contextReadError(ctx, nil); contextErr != nil {
		return contextErr
	}
	if !validOpaque(token, 16_384) {
		return ErrCredentialExpired
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, client.baseURL+path, nil)
	if err != nil {
		return ErrInvalidInput
	}
	request.Header.Set("Authorization", "Bearer "+token)
	request.Header.Set("Prefer", `outlook.timezone="UTC"`)
	response, err := client.http.Do(request)
	if err != nil {
		if contextErr := contextReadError(ctx, err); contextErr != nil {
			return contextErr
		}
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

func contextReadError(ctx context.Context, err error) error {
	if contextErr := ctx.Err(); contextErr != nil {
		return contextErr
	}
	if errors.Is(err, context.Canceled) {
		return context.Canceled
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return context.DeadlineExceeded
	}
	return nil
}

func (client *Client) nextCursor(raw string) (string, error) {
	if raw == "" {
		return "", nil
	}
	next, err := url.Parse(raw)
	base, baseErr := url.Parse(client.baseURL)
	if err != nil || baseErr != nil || next.Scheme != base.Scheme || next.Host != base.Host || !strings.HasSuffix(next.Path, "/calendarView") {
		return "", ErrInvalidResponse
	}
	for _, name := range []string{"$skiptoken", "$skip"} {
		if value := next.Query().Get(name); validOpaque(value, 1900) {
			return strings.TrimPrefix(name, "$") + ":" + value, nil
		}
	}
	return "", ErrInvalidResponse
}

func decodeCursor(cursor string) (string, string, bool) {
	if len(cursor) > 2048 || strings.ContainsAny(cursor, "\r\n\x00") {
		return "", "", false
	}
	prefix, value, found := strings.Cut(cursor, ":")
	if !found || !validOpaque(value, 1900) {
		return "", "", false
	}
	switch prefix {
	case "skiptoken":
		return "$skiptoken", value, true
	case "skip":
		return "$skip", value, true
	default:
		return "", "", false
	}
}

func parseEventTime(value eventDateTime) (time.Time, error) {
	if !validOpaque(value.DateTime, 64) || !strings.EqualFold(value.TimeZone, "UTC") {
		return time.Time{}, ErrInvalidResponse
	}
	if strings.HasSuffix(value.DateTime, "Z") || len(value.DateTime) >= 10 && strings.ContainsAny(value.DateTime[10:], "+-") {
		return time.Parse(time.RFC3339Nano, value.DateTime)
	}
	return time.ParseInLocation("2006-01-02T15:04:05.9999999", value.DateTime, time.UTC)
}

func validOpaque(value string, maximum int) bool {
	return strings.TrimSpace(value) != "" && len(value) <= maximum && !strings.ContainsAny(value, "\r\n\x00")
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
