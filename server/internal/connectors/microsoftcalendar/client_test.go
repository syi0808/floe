package microsoftcalendar

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

type tokenSource struct {
	token string
	err   error
}

func (source tokenSource) Token(context.Context) (string, error) { return source.token, source.err }

func TestCalendarReadsOnlyBoundedSelectedCalendarProjection(t *testing.T) {
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.URL.EscapedPath() != "/me/calendars/team%2Fselected/calendarView" || request.Header.Get("Authorization") != "Bearer private-token" || request.Header.Get("Prefer") != `outlook.timezone="UTC"` {
			t.Fatalf("unexpected request: %s %s %#v", request.Method, request.URL.EscapedPath(), request.Header)
		}
		query := request.URL.Query()
		if query.Get("$top") != "25" || query.Get("$skiptoken") != "opaque-page" || query.Get("$select") != "id,subject,start,end,isAllDay,isCancelled" || strings.Contains(request.URL.RawQuery, "provider-event-id") {
			t.Fatalf("unexpected query: %s", request.URL.RawQuery)
		}
		fmt.Fprintf(writer, `{"value":[{"id":"provider-event-id","subject":"Planning review","start":{"dateTime":"2026-09-11T13:00:00.0000000","timeZone":"UTC"},"end":{"dateTime":"2026-09-11T14:00:00.0000000","timeZone":"UTC"},"isAllDay":false,"isCancelled":false,"bodyPreview":"ignored body","location":{"displayName":"ignored location"},"attendees":[{"emailAddress":{"address":"ignored@example.com"}}]}],"@odata.nextLink":%q}`, server.URL+`/me/calendars/team%2Fselected/calendarView?%24skiptoken=next-page`)
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "team/selected", "account-1")
	if err != nil {
		t.Fatal(err)
	}
	view, err := client.Calendar(context.Background(), time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC), time.Date(2026, 9, 12, 0, 0, 0, 0, time.UTC), "skiptoken:opaque-page", 25, time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC))
	if err != nil || len(view.Items) != 1 || view.Items[0].UntrustedTitle != "Planning review" || view.NextCursor == nil || *view.NextCursor != "skiptoken:next-page" {
		t.Fatalf("view: %#v %v", view, err)
	}
	encoded, err := json.Marshal(view)
	if err != nil || strings.Contains(string(encoded), "provider-event-id") || strings.Contains(string(encoded), "ignored body") || strings.Contains(string(encoded), "ignored@example.com") || strings.Contains(string(encoded), "team/selected") || strings.Contains(string(encoded), "private-token") {
		t.Fatalf("view leaked provider data: %s %v", encoded, err)
	}
}

func TestCalendarFailuresAndUntrustedPaginationAreTyped(t *testing.T) {
	for _, test := range []struct {
		status int
		want   error
	}{{http.StatusUnauthorized, ErrCredentialExpired}, {http.StatusForbidden, ErrPermissionDenied}, {http.StatusTooManyRequests, ErrRateLimited}, {http.StatusBadGateway, ErrUnavailable}} {
		t.Run(fmt.Sprint(test.status), func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) { writer.WriteHeader(test.status) }))
			defer server.Close()
			client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "selected", "account-1")
			_, err := client.Calendar(context.Background(), time.Now(), time.Now().Add(time.Hour), "", 1, time.Now())
			if !errors.Is(err, test.want) {
				t.Fatalf("got %v", err)
			}
		})
	}
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		fmt.Fprint(writer, `{"value":[],"@odata.nextLink":"https://evil.example/calendarView?$skiptoken=secret"}`)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "selected", "account-1")
	if _, err := client.Calendar(context.Background(), time.Now(), time.Now().Add(time.Hour), "", 1, time.Now()); !errors.Is(err, ErrInvalidResponse) {
		t.Fatalf("untrusted pagination: %v", err)
	}
}

func TestRejectsUntrustedEndpointsInputsAndResponses(t *testing.T) {
	for _, endpoint := range []string{"http://example.com/v1.0", "https://user@example.com/v1.0", "https://example.com/v1.0?token=x", "https://example.com/not-graph"} {
		if _, err := NewWithBaseURL(tokenSource{token: "private-token"}, endpoint, "selected", "account-1"); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted %q: %v", endpoint, err)
		}
	}
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, "http://127.0.0.1:1", "selected", "account-1")
	now := time.Now()
	for _, cursor := range []string{"unknown:value", "skiptoken:bad\nvalue"} {
		if _, err := client.Calendar(context.Background(), now, now.Add(time.Hour), cursor, 1, now); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted cursor %q: %v", cursor, err)
		}
	}
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		fmt.Fprint(writer, `{"value":[{"id":"event","subject":"Bad","start":{"dateTime":"2026-09-11T14:00:00","timeZone":"Pacific Standard Time"},"end":{"dateTime":"2026-09-11T15:00:00","timeZone":"Pacific Standard Time"}}]}`)
	}))
	defer server.Close()
	invalid, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "selected", "account-1")
	if _, err := invalid.Calendar(context.Background(), time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC), time.Date(2026, 9, 12, 0, 0, 0, 0, time.UTC), "", 1, time.Now()); !errors.Is(err, ErrInvalidResponse) {
		t.Fatalf("invalid timezone: %v", err)
	}
}
