package googlecalendar

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"floe/server/internal/connectors/common"
)

func TestServicePublishesTypedFailureAndFreshCache(t *testing.T) {
	rateLimited := false
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		if rateLimited {
			writer.WriteHeader(http.StatusTooManyRequests)
			return
		}
		fmt.Fprint(writer, `{"items":[{"id":"event","status":"confirmed","summary":"Review","start":{"dateTime":"2026-09-11T13:00:00Z"},"end":{"dateTime":"2026-09-11T14:00:00Z"}}]}`)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "selected", "account-1")
	service, _ := NewService(client)
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	service.clock = func() time.Time { return now }
	if _, err := service.ConnectionSnapshot(context.Background()); err != nil {
		t.Fatal(err)
	}
	rateLimited = true
	snapshot, err := service.ConnectionSnapshot(context.Background())
	value := snapshot.(common.Snapshot)
	if err != nil || value.Connection.State != "degraded" || value.Connection.LastFailure.Kind != "rate_limited" || len(value.Views) != 1 {
		t.Fatalf("snapshot: %#v %v", value, err)
	}
}

func TestServiceReadsBothConfiguredCalendarsWithoutEvidenceCollision(t *testing.T) {
	seen := []string{}
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		seen = append(seen, request.URL.EscapedPath())
		if request.Header.Get("Authorization") != "Bearer private-token" {
			t.Fatal("missing credential")
		}
		fmt.Fprint(writer, `{"items":[{"id":"provider-shared-id","status":"confirmed","summary":"Review","start":{"dateTime":"2026-09-11T13:00:00Z"},"end":{"dateTime":"2026-09-11T14:00:00Z"}}]}`)
	}))
	defer server.Close()
	clients := make([]*Client, 2)
	for index, calendarID := range []string{"calendar-a", "calendar-b"} {
		client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, calendarID, "connection")
		if err != nil {
			t.Fatal(err)
		}
		clients[index] = client
	}
	service, err := NewService(clients...)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	service.clock = func() time.Time { return now }
	viewValue, err := service.ReadCalendarView(context.Background(), now.Add(-time.Hour), now.Add(24*time.Hour), "", 2)
	if err != nil {
		t.Fatal(err)
	}
	view := viewValue.(CalendarView)
	if !view.CoverageComplete || len(view.Items) != 2 || view.Items[0].EvidenceHandle == view.Items[1].EvidenceHandle || fmt.Sprint(seen) != "[/calendars/calendar-a/events /calendars/calendar-b/events]" {
		t.Fatalf("multi-calendar view: %+v paths=%v", view, seen)
	}
	encoded, _ := json.Marshal(view)
	for _, secret := range []string{"calendar-a", "calendar-b", "provider-shared-id", "private-token"} {
		if strings.Contains(string(encoded), secret) {
			t.Fatalf("provider value leaked: %s", secret)
		}
	}
}
