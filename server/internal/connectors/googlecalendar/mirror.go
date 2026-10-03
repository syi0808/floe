package googlecalendar

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"

	"floe/server/internal/views"
    "floe/server/internal/trust"
)

const mirrorMaxResponse = 1 << 20

type mirrorClientKey struct {
	connectionID string
	calendarID   string
}

type mirrorReader struct {
	clients map[mirrorClientKey]*Client
}

type mirrorPage struct {
	TimeZone      string        `json:"timeZone"`
	NextPageToken string        `json:"nextPageToken"`
	Items         []mirrorEvent `json:"items"`
}

type mirrorEvent struct {
	ID      string          `json:"id"`
	ETag    string          `json:"etag"`
	Status  string          `json:"status"`
	Summary string          `json:"summary"`
	Start   mirrorDateTime  `json:"start"`
	End     mirrorDateTime  `json:"end"`
}

type mirrorDateTime struct {
	DateTime string `json:"dateTime"`
	Date     string `json:"date"`
	TimeZone string `json:"timeZone"`
}

// NewMirrorReader builds a reader over immutable, constructor-selected
// provider clients. A read is bound to both the source connection and calendar.
func NewMirrorReader(clients ...*Client) (views.Reader, error) {
	if len(clients) == 0 {
		return nil, ErrInvalidInput
	}
	bySource := make(map[mirrorClientKey]*Client, len(clients))
	for _, client := range clients {
		if client == nil || client.tokens == nil || client.http == nil || client.baseURL == "" ||
			!validOpaque(client.calendarID, 512) || !validOpaque(client.connectionID, 128) {
			return nil, ErrInvalidInput
		}
		key := mirrorClientKey{connectionID: client.connectionID, calendarID: client.calendarID}
		if _, exists := bySource[key]; exists {
			return nil, ErrInvalidInput
		}
		bySource[key] = client
	}
	return &mirrorReader{clients: bySource}, nil
}

func (reader *mirrorReader) Read(ctx context.Context, request views.ReadRequest) (views.Result, error) {
	if err := mirrorContextFailure(ctx, nil); err != nil {
		return views.Result{}, err
	}
	if ctx == nil || request.Query.ViewID != views.CalendarMirror || request.Query.Mirror == nil ||
		request.Query.Calendar != nil || request.Query.Mail != nil || request.Query.Work != nil || request.Query.Logistics != nil {
		return mirrorReadError(views.InvalidQuery)
	}

	queryJSON, err := json.Marshal(request.Query.Mirror)
	if err != nil {
		return mirrorReadError(views.InvalidQuery)
	}
	query, err := views.ParseMirrorQuery(queryJSON)
	if err != nil || request.Bounds.MaxItems == 0 || request.Bounds.MaxItems > maxItems ||
		request.Bounds.MaxBytes == 0 || request.Bounds.MaxBytes > mirrorMaxResponse ||
		uint32(query.Limit) > request.Bounds.MaxItems || !validMirrorCursor(request.ProviderCursor, 4096) {
		return mirrorReadError(views.InvalidQuery)
	}

	client := reader.clients[mirrorClientKey{connectionID: request.Source.ConnectionID, calendarID: query.CalendarID}]
	if client == nil || !sourceHasCalendar(request.Source.Resources, query.CalendarID) {
		return mirrorReadError(views.InvalidQuery)
	}

	page, err := client.readMirrorPage(ctx, query, request.ProviderCursor)
	if err != nil {
		return classifyMirrorError(ctx, err)
	}
	now := time.Now()
	if now.UnixMilli() <= 0 {
		return mirrorReadError(views.Unavailable)
	}
	records := make([]views.CalendarRecord, 0, len(page.Items))
	seen := make(map[string]struct{}, len(page.Items))
	for _, event := range page.Items {
		if contextErr := mirrorContextFailure(ctx, nil); contextErr != nil {
			return views.Result{}, contextErr
		}
		if !validMirrorText(event.ID, 512) {
			return mirrorReadError(views.InvalidProviderResponse)
		}
		if _, exists := seen[event.ID]; exists {
			return mirrorReadError(views.InvalidProviderResponse)
		}
		seen[event.ID] = struct{}{}
		if event.Status == "cancelled" {
			continue
		}
		if event.Status != "confirmed" && event.Status != "tentative" || len(event.Summary) > 4096 || !utf8.ValidString(event.Summary) {
			return mirrorReadError(views.InvalidProviderResponse)
		}
		schedule, scheduleErr := googleMirrorSchedule(event, page.TimeZone)
		if scheduleErr != nil {
			return mirrorReadError(views.InvalidProviderResponse)
		}
		record := views.CalendarRecord{
			CanModify: false,
			CalendarID: query.CalendarID,
			ExternalID: event.ID,
			Title:     event.Summary,
			Schedule:  schedule,
		}
		if event.ETag != "" {
			if !validMirrorText(event.ETag, 512) {
				return mirrorReadError(views.InvalidProviderResponse)
			}
			record.ExternalRevision = views.CalendarExternalRevision{Kind: "provider_opaque", Value: event.ETag}
		} else {
			record.ExternalRevision = views.ObservationRevision(record)
		}
		records = append(records, record)
	}
	if contextErr := mirrorContextFailure(ctx, nil); contextErr != nil {
		return views.Result{}, contextErr
	}
	result := views.CalendarMirrorResult{
		CalendarID:       query.CalendarID,
		RangeStartUnixMS: query.RangeStartUnixMS,
		RangeEndUnixMS:   query.RangeEndUnixMS,
		ObservedAtUnixMS: now.UnixMilli(),
		ExpiresAtUnixMS:  now.Add(5 * time.Minute).UnixMilli(),
		Records:          records,
		NextCursor:       page.NextPageToken,
	}
	if err := views.ValidateMirrorResult(result, request); err != nil {
		return mirrorReadError(views.InvalidProviderResponse)
	}
	return views.Result{ViewID: views.CalendarMirror, Mirror: &result}, nil
}

func (client *Client) readMirrorPage(ctx context.Context, query views.CalendarMirrorQuery, cursor string) (mirrorPage, error) {
	if !validMirrorCursor(cursor, 4096) {
		return mirrorPage{}, ErrInvalidInput
	}
	values := url.Values{
		"singleEvents": {"true"},
		"orderBy":      {"startTime"},
		"timeMin":      {time.UnixMilli(query.RangeStartUnixMS).UTC().Format(time.RFC3339Nano)},
		"timeMax":      {time.UnixMilli(query.RangeEndUnixMS).UTC().Format(time.RFC3339Nano)},
		"maxResults":   {strconv.FormatUint(uint64(query.Limit), 10)},
		"fields":       {"items(id,etag,status,summary,start(date,dateTime,timeZone),end(date,dateTime,timeZone)),timeZone,nextPageToken"},
	}
	if cursor != "" {
		values.Set("pageToken", cursor)
	}
	path := "/calendars/" + url.PathEscape(client.calendarID) + "/events?" + values.Encode()
	var page mirrorPage
	if err := client.getMirror(ctx, path, &page); err != nil {
		return mirrorPage{}, err
	}
	if page.Items==nil || !validMirrorText(page.TimeZone,128) || len(page.Items) > int(query.Limit) || !validMirrorCursor(page.NextPageToken, 4096) {
		return mirrorPage{}, ErrInvalidResponse
	}
	return page, nil
}

func (client *Client) getMirror(ctx context.Context, path string, output any) error {
	token, err := client.tokens.Token(ctx)
	if contextErr := mirrorContextFailure(ctx, err); contextErr != nil {
		return contextErr
	}
	if err != nil || !validOpaque(token, 16_384) {
		return ErrCredentialExpired
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, client.baseURL+path, nil)
	if err != nil {
		return ErrInvalidInput
	}
	request.Header.Set("Authorization", "Bearer "+token)
	response, err := client.http.Do(request)
    if response!=nil && response.Body!=nil {defer response.Body.Close()}
	if contextErr := mirrorContextFailure(ctx, err); contextErr != nil {
		return contextErr
	}
	if err != nil {
		return ErrUnavailable
	}
    if response==nil || response.Body==nil{return ErrInvalidResponse}
	if contextErr := mirrorContextFailure(ctx, nil); contextErr != nil {
		return contextErr
	}
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
	data, err := io.ReadAll(io.LimitReader(response.Body, mirrorMaxResponse+1))
	if contextErr := mirrorContextFailure(ctx, err); contextErr != nil {
		return contextErr
	}
	if err != nil || len(data) > mirrorMaxResponse || !utf8.Valid(data) || trust.StrictJSON(data,mirrorMaxResponse,32)!=nil || json.Unmarshal(data, output) != nil {
		return ErrInvalidResponse
	}
	return nil
}

func googleMirrorSchedule(event mirrorEvent, calendarTimeZone string) (views.CalendarSchedule, error) {
	startAllDay := event.Start.Date != ""
	endAllDay := event.End.Date != ""
	if startAllDay || endAllDay {
		if !startAllDay || !endAllDay || event.Start.DateTime != "" || event.End.DateTime != "" ||
			!validGoogleDate(event.Start.Date) || !validGoogleDate(event.End.Date) || event.Start.Date >= event.End.Date {
			return views.CalendarSchedule{}, ErrInvalidResponse
		}
		return views.CalendarSchedule{Kind: "all_day", StartDate: event.Start.Date, EndDateExclusive: event.End.Date}, nil
	}
	if event.Start.DateTime == "" || event.End.DateTime == "" || event.Start.Date != "" || event.End.Date != "" {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}
	start, startErr := time.Parse(time.RFC3339Nano, event.Start.DateTime)
	end, endErr := time.Parse(time.RFC3339Nano, event.End.DateTime)
	if startErr != nil || endErr != nil || !start.Before(end) {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}
	timeZone, err := googleMirrorTimeZone(event.Start.TimeZone, event.End.TimeZone, calendarTimeZone)
	if err != nil {
		return views.CalendarSchedule{}, err
	}
	return views.CalendarSchedule{
		Kind:     "timed",
		StartsAt: start.UTC().Format(time.RFC3339Nano),
		EndsAt:   end.UTC().Format(time.RFC3339Nano),
		Timezone: timeZone,
	}, nil
}

func googleMirrorTimeZone(start, end, calendar string) (string, error) {
	if start != "" && !validMirrorText(start, 128) || end != "" && !validMirrorText(end, 128) || calendar != "" && !validMirrorText(calendar, 128) {
		return "", ErrInvalidResponse
	}
	if start != "" {
		return start, nil
	}
	if calendar == "" {
		return "", ErrInvalidResponse
	}
	return calendar, nil
}

func validGoogleDate(value string) bool {
	parsed, err := time.Parse("2006-01-02", value)
	return err == nil && parsed.Format("2006-01-02") == value
}

func validMirrorCursor(value string, maximum int) bool {
	return value == "" || len(value) <= maximum && utf8.ValidString(value) && strings.TrimSpace(value) != "" && !strings.ContainsFunc(value, unicode.IsControl)
}

func validMirrorText(value string, maximum int) bool {
	return strings.TrimSpace(value) != "" && len(value) <= maximum && utf8.ValidString(value) && !strings.ContainsFunc(value, unicode.IsControl)
}

func sourceHasCalendar(resources []string, calendarID string) bool {
	for _, resource := range resources {
		if resource == calendarID {
			return true
		}
	}
	return false
}

func mirrorReadError(kind views.ReadErrorKind) (views.Result, error) {
	return views.Result{}, views.ReadError{Kind: kind}
}

func mirrorContextFailure(ctx context.Context, err error) error {
	if ctx != nil {
		if contextErr := ctx.Err(); contextErr != nil {
			return contextErr
		}
	}
	if errors.Is(err, context.Canceled) {
		return context.Canceled
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return context.DeadlineExceeded
	}
	return nil
}

func classifyMirrorError(ctx context.Context, err error) (views.Result, error) {
	if contextErr := mirrorContextFailure(ctx, err); contextErr != nil {
		return views.Result{}, contextErr
	}
	switch {
	case errors.Is(err, ErrInvalidInput):
		return mirrorReadError(views.InvalidQuery)
	case errors.Is(err, ErrCredentialExpired):
		return mirrorReadError(views.CredentialExpired)
	case errors.Is(err, ErrPermissionDenied):
		return mirrorReadError(views.PermissionDenied)
	case errors.Is(err, ErrRateLimited):
		return mirrorReadError(views.RateLimited)
	case errors.Is(err, ErrInvalidResponse):
		return mirrorReadError(views.InvalidProviderResponse)
	default:
		return mirrorReadError(views.Unavailable)
	}
}
