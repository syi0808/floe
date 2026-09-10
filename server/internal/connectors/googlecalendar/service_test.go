package googlecalendar

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
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
