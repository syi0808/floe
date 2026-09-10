package googlecalendar

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
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.URL.EscapedPath() != "/calendars/team%2Fselected/events" || request.Header.Get("Authorization") != "Bearer private-token" {
			t.Fatalf("unexpected request: %s %s %#v", request.Method, request.URL.EscapedPath(), request.Header)
		}
		query := request.URL.Query()
		if query.Get("singleEvents") != "true" || query.Get("orderBy") != "startTime" || query.Get("timeZone") != "UTC" || query.Get("maxResults") != "25" || query.Get("pageToken") != "cursor-1" || strings.Contains(request.URL.RawQuery, "provider-event-id") {
			t.Fatalf("unexpected query: %s", request.URL.RawQuery)
		}
		fmt.Fprint(writer, `{"items":[{"id":"provider-event-id","status":"confirmed","summary":"Planning review","start":{"dateTime":"2026-09-11T13:00:00Z"},"end":{"dateTime":"2026-09-11T14:00:00Z"},"description":"ignored private description","location":"ignored location","attendees":[{"email":"ignored@example.com"}]}],"nextPageToken":"next-cursor"}`)
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "team/selected", "account-1")
	if err != nil {
		t.Fatal(err)
	}
	view, err := client.Calendar(context.Background(), time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC), time.Date(2026, 9, 12, 0, 0, 0, 0, time.UTC), "cursor-1", 25, time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC))
	if err != nil || len(view.Items) != 1 || view.Items[0].UntrustedTitle != "Planning review" || view.CoverageComplete || view.NextCursor == nil || *view.NextCursor != "next-cursor" {
		t.Fatalf("view: %#v %v", view, err)
	}
	encoded, err := json.Marshal(view)
	if err != nil || strings.Contains(string(encoded), "provider-event-id") || strings.Contains(string(encoded), "private description") || strings.Contains(string(encoded), "ignored@example.com") || strings.Contains(string(encoded), "team/selected") || strings.Contains(string(encoded), "private-token") {
		t.Fatalf("view leaked provider data: %s %v", encoded, err)
	}
}

func TestCalendarParsesAllDayAndSkipsCancelledEvents(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		fmt.Fprint(writer, `{"items":[{"id":"all-day","status":"tentative","summary":"Offsite","start":{"date":"2026-09-11"},"end":{"date":"2026-09-12"}},{"id":"deleted","status":"cancelled"}]}`)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "selected", "account-1")
	view, err := client.Calendar(context.Background(), time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC), time.Date(2026, 9, 13, 0, 0, 0, 0, time.UTC), "", 10, time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC))
	if err != nil || len(view.Items) != 1 || !view.Items[0].AllDay {
		t.Fatalf("view: %#v %v", view, err)
	}
}

func TestCalendarFailuresAreTyped(t *testing.T) {
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
}

func TestRejectsUntrustedEndpointsInputsAndResponses(t *testing.T) {
	for _, endpoint := range []string{"http://example.com/calendar/v3", "https://user@example.com/calendar/v3", "https://example.com/calendar/v3?token=x", "https://example.com/not-calendar"} {
		if _, err := NewWithBaseURL(tokenSource{token: "private-token"}, endpoint, "selected", "account-1"); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted %q: %v", endpoint, err)
		}
	}
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, "http://127.0.0.1:1", "selected", "account-1")
	now := time.Now()
	for _, input := range []struct {
		start, end time.Time
		cursor     string
		limit      int
	}{{now, now, "", 1}, {now, now.Add(33 * 24 * time.Hour), "", 1}, {now, now.Add(time.Hour), "bad\ncursor", 1}, {now, now.Add(time.Hour), "", 0}} {
		if _, err := client.Calendar(context.Background(), input.start, input.end, input.cursor, input.limit, now); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted input: %#v %v", input, err)
		}
	}
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		fmt.Fprint(writer, `{"items":[{"id":"event","status":"confirmed","start":{"dateTime":"2026-09-11T14:00:00Z"},"end":{"dateTime":"2026-09-11T13:00:00Z"}}]}`)
	}))
	defer server.Close()
	invalid, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "selected", "account-1")
	if _, err := invalid.Calendar(context.Background(), time.Date(2026, 9, 11, 0, 0, 0, 0, time.UTC), time.Date(2026, 9, 12, 0, 0, 0, 0, time.UTC), "", 1, time.Now()); !errors.Is(err, ErrInvalidResponse) {
		t.Fatalf("invalid response: %v", err)
	}
}
