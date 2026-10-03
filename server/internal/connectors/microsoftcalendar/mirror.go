package microsoftcalendar

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

	"floe/server/internal/trust"
	"floe/server/internal/views"
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
	NextLink   string        `json:"@odata.nextLink"`
	Value      []mirrorEvent `json:"value"`
	NextCursor string        `json:"-"`
}

type mirrorEvent struct {
	ID                    string         `json:"id"`
	ETag                  string         `json:"@odata.etag"`
	ChangeKey             string         `json:"changeKey"`
	Subject               string         `json:"subject"`
	Start                 mirrorDateTime `json:"start"`
	End                   mirrorDateTime `json:"end"`
	IsAllDay              bool           `json:"isAllDay"`
	IsCancelled           bool           `json:"isCancelled"`
	OriginalStartTimeZone string         `json:"originalStartTimeZone"`
	OriginalEndTimeZone   string         `json:"originalEndTimeZone"`
}

type mirrorDateTime struct {
	DateTime string `json:"dateTime"`
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
	records := make([]views.CalendarRecord, 0, len(page.Value))
	seen := make(map[string]struct{}, len(page.Value))
	for _, event := range page.Value {
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
		if event.IsCancelled {
			continue
		}
		if len(event.Subject) > 4096 || !utf8.ValidString(event.Subject) {
			return mirrorReadError(views.InvalidProviderResponse)
		}
		var schedule views.CalendarSchedule
		if event.IsAllDay {
			schedule, err = client.readAllDayMirrorSchedule(ctx, event)
		} else {
			schedule, err = microsoftTimedMirrorSchedule(event)
		}
		if err != nil {
			return classifyMirrorError(ctx, err)
		}
		record := views.CalendarRecord{
			CanModify:  false,
			CalendarID: query.CalendarID,
			ExternalID: event.ID,
			Title:      event.Subject,
			Schedule:   schedule,
		}
		record.ExternalRevision, err = microsoftMirrorRevision(record, event)
		if err != nil {
			return classifyMirrorError(ctx, err)
		}
		records = append(records, record)
	}
	if contextErr := mirrorContextFailure(ctx, nil); contextErr != nil {
		return views.Result{}, contextErr
	}
	now := time.Now()
	if now.UnixMilli() <= 0 {
		return mirrorReadError(views.Unavailable)
	}
	result := views.CalendarMirrorResult{
		CalendarID:       query.CalendarID,
		RangeStartUnixMS: query.RangeStartUnixMS,
		RangeEndUnixMS:   query.RangeEndUnixMS,
		ObservedAtUnixMS: now.UnixMilli(),
		ExpiresAtUnixMS:  now.Add(5 * time.Minute).UnixMilli(),
		Records:          records,
		NextCursor:       page.NextCursor,
	}
	if err := views.ValidateMirrorResult(result, request); err != nil {
		return mirrorReadError(views.InvalidProviderResponse)
	}
	return views.Result{ViewID: views.CalendarMirror, Mirror: &result}, nil
}

func (client *Client) readMirrorPage(ctx context.Context, query views.CalendarMirrorQuery, cursor string) (mirrorPage, error) {
	values := url.Values{
		"startDateTime": {time.UnixMilli(query.RangeStartUnixMS).UTC().Format(time.RFC3339Nano)},
		"endDateTime":   {time.UnixMilli(query.RangeEndUnixMS).UTC().Format(time.RFC3339Nano)},
		"$top":          {strconv.FormatUint(uint64(query.Limit), 10)},
		"$select":       {"id,changeKey,subject,start,end,isAllDay,isCancelled,originalStartTimeZone,originalEndTimeZone"},
	}
	if cursor != "" {
		name, value, ok := decodeMirrorCursor(cursor)
		if !ok {
			return mirrorPage{}, ErrInvalidInput
		}
		values.Set(name, value)
	}
	path := "/me/calendars/" + url.PathEscape(client.calendarID) + "/calendarView?" + values.Encode()
	var page mirrorPage
	if err := client.getMirror(ctx, path, "UTC", &page); err != nil {
		return mirrorPage{}, err
	}
	if page.Value == nil || len(page.Value) > int(query.Limit) {
		return mirrorPage{}, ErrInvalidResponse
	}
	nextCursor, err := client.mirrorNextCursor(page.NextLink)
	if err != nil {
		return mirrorPage{}, err
	}
	page.NextCursor = nextCursor
	return page, nil
}

func (client *Client) readAllDayMirrorSchedule(ctx context.Context, listed mirrorEvent) (views.CalendarSchedule, error) {
	timeZone := listed.OriginalStartTimeZone
	if !validMicrosoftTimeZone(timeZone) || listed.OriginalEndTimeZone != timeZone ||
		!validMicrosoftTimeZone(listed.OriginalEndTimeZone) || validateMicrosoftRevisions(listed) != nil || !hasMicrosoftRevision(listed) {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}

	values := url.Values{
		"$select": {"id,changeKey,isAllDay,isCancelled,start,end,originalStartTimeZone,originalEndTimeZone"},
	}
	path := "/me/calendars/" + url.PathEscape(client.calendarID) + "/events/" + url.PathEscape(listed.ID) + "?" + values.Encode()
	var detail mirrorEvent
	if err := client.getMirror(ctx, path, timeZone, &detail); err != nil {
		return views.CalendarSchedule{}, err
	}
	if detail.ID != listed.ID || !detail.IsAllDay || detail.IsCancelled ||
		!sameMicrosoftRevision(listed, detail) || detail.OriginalStartTimeZone != timeZone || detail.OriginalEndTimeZone != timeZone ||
		!strings.EqualFold(detail.Start.TimeZone, timeZone) || !strings.EqualFold(detail.End.TimeZone, timeZone) {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}
	startDate, err := microsoftCivilDate(detail.Start.DateTime)
	if err != nil {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}
	endDate, err := microsoftCivilDate(detail.End.DateTime)
	if err != nil || startDate >= endDate {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}
	return views.CalendarSchedule{Kind: "all_day", StartDate: startDate, EndDateExclusive: endDate}, nil
}

func (client *Client) getMirror(ctx context.Context, path, timeZone string, output any) error {
	token, err := client.tokens.Token(ctx)
	if contextErr := mirrorContextFailure(ctx, err); contextErr != nil {
		return contextErr
	}
	if err != nil || !validOpaque(token, 16_384) {
		return ErrCredentialExpired
	}
	if !validMicrosoftTimeZone(timeZone) {
		return ErrInvalidInput
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, client.baseURL+path, nil)
	if err != nil {
		return ErrInvalidInput
	}
	request.Header.Set("Authorization", "Bearer "+token)
	request.Header.Set("Prefer", `outlook.timezone="`+timeZone+`"`)
	response, err := client.http.Do(request)
	if response != nil && response.Body != nil {
		defer response.Body.Close()
	}
	if contextErr := mirrorContextFailure(ctx, err); contextErr != nil {
		return contextErr
	}
	if err != nil {
		return ErrUnavailable
	}
	if response == nil || response.Body == nil {
		return ErrInvalidResponse
	}
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
	if err != nil || len(data) > mirrorMaxResponse || !utf8.Valid(data) || trust.StrictJSON(data, mirrorMaxResponse, 32) != nil || json.Unmarshal(data, output) != nil {
		return ErrInvalidResponse
	}
	return nil
}

func (client *Client) mirrorNextCursor(raw string) (string, error) {
	if raw == "" {
		return "", nil
	}
	next, err := url.Parse(raw)
	base, baseErr := url.Parse(client.baseURL)
	if err != nil || baseErr != nil || next.User != nil || next.Fragment != "" || !next.IsAbs() ||
		!strings.EqualFold(next.Scheme, base.Scheme) || !strings.EqualFold(next.Host, base.Host) ||
		next.EscapedPath() != client.mirrorCalendarViewEscapedPath() {
		return "", ErrInvalidResponse
	}
	query, err := url.ParseQuery(next.RawQuery)
	if err != nil {
		return "", ErrInvalidResponse
	}
	var cursorName, cursorValue string
	for _, name := range []string{"$skiptoken", "$skip"} {
		values, exists := query[name]
		if !exists {
			continue
		}
		if cursorName != "" || len(values) != 1 || !validMirrorCursor(values[0], 4096) || values[0] == "" {
			return "", ErrInvalidResponse
		}
		cursorName = name
		cursorValue = values[0]
	}
	if cursorName == "" {
		return "", ErrInvalidResponse
	}
	prefix := "skiptoken:"
	if cursorName == "$skip" {
		prefix = "skip:"
	}
	cursor := prefix + cursorValue
	if len(cursor) > 4096 {
		return "", ErrInvalidResponse
	}
	return cursor, nil
}

func (client *Client) mirrorCalendarViewEscapedPath() string {
	base, err := url.Parse(client.baseURL)
	if err != nil {
		return ""
	}
	return strings.TrimSuffix(base.EscapedPath(), "/") + "/me/calendars/" + url.PathEscape(client.calendarID) + "/calendarView"
}

func microsoftTimedMirrorSchedule(event mirrorEvent) (views.CalendarSchedule, error) {
	timeZone := event.OriginalStartTimeZone
	if !validMicrosoftTimeZone(timeZone) || event.OriginalEndTimeZone != "" && !validMicrosoftTimeZone(event.OriginalEndTimeZone) ||
		!strings.EqualFold(event.Start.TimeZone, "UTC") || !strings.EqualFold(event.End.TimeZone, "UTC") {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}
	start, startErr := microsoftDateTime(event.Start.DateTime)
	end, endErr := microsoftDateTime(event.End.DateTime)
	if startErr != nil || endErr != nil || start.UnixMilli() < 0 || !start.Before(end) {
		return views.CalendarSchedule{}, ErrInvalidResponse
	}
	return views.CalendarSchedule{
		Kind:     "timed",
		StartsAt: start.UTC().Format(time.RFC3339Nano),
		EndsAt:   end.UTC().Format(time.RFC3339Nano),
		Timezone: timeZone,
	}, nil
}

func microsoftDateTime(value string) (time.Time, error) {
	if !validMirrorText(value, 64) {
		return time.Time{}, ErrInvalidResponse
	}
	parsed, err := time.Parse(time.RFC3339Nano, value)
	if err == nil {
		return parsed, nil
	}
	parsed, err = time.ParseInLocation("2006-01-02T15:04:05.999999999", value, time.UTC)
	if err != nil {
		return time.Time{}, ErrInvalidResponse
	}
	return parsed, nil
}

func microsoftCivilDate(value string) (string, error) {
	parsed, err := microsoftDateTime(value)
	if err != nil || parsed.Hour() != 0 || parsed.Minute() != 0 || parsed.Second() != 0 || parsed.Nanosecond() != 0 || len(value) < 10 {
		return "", ErrInvalidResponse
	}
	date := value[:10]
	parsedDate, err := time.Parse("2006-01-02", date)
	if err != nil || parsedDate.Format("2006-01-02") != date {
		return "", ErrInvalidResponse
	}
	return date, nil
}

func microsoftMirrorRevision(record views.CalendarRecord, event mirrorEvent) (views.CalendarExternalRevision, error) {
	if err := validateMicrosoftRevisions(event); err != nil {
		return views.CalendarExternalRevision{}, err
	}
	if event.ETag != "" {
		return views.CalendarExternalRevision{Kind: "provider_opaque", Value: event.ETag}, nil
	}
	if event.ChangeKey != "" {
		return views.CalendarExternalRevision{Kind: "provider_opaque", Value: event.ChangeKey}, nil
	}
	return views.ObservationRevision(record), nil
}

func hasMicrosoftRevision(event mirrorEvent) bool {
	return event.ETag != "" || event.ChangeKey != ""
}

func sameMicrosoftRevision(left, right mirrorEvent) bool {
	return validateMicrosoftRevisions(left) == nil && validateMicrosoftRevisions(right) == nil &&
		hasMicrosoftRevision(left) && left.ETag == right.ETag && left.ChangeKey == right.ChangeKey
}

func validateMicrosoftRevisions(event mirrorEvent) error {
	if event.ETag != "" && !validMirrorText(event.ETag, 512) || event.ChangeKey != "" && !validMirrorText(event.ChangeKey, 512) {
		return ErrInvalidResponse
	}
	return nil
}

func decodeMirrorCursor(cursor string) (string, string, bool) {
	if cursor == "" {
		return "", "", true
	}
	if !validMirrorCursor(cursor, 4096) {
		return "", "", false
	}
	prefix, value, found := strings.Cut(cursor, ":")
	if !found || value == "" || !validMirrorCursor(value, 4086) {
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

func validMicrosoftTimeZone(value string) bool {
	return validMirrorText(value, 128) && strings.TrimSpace(value) == value && !strings.ContainsAny(value, "\"\\")
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
