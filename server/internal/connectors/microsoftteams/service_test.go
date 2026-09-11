package microsoftteams

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"floe/server/internal/connectors/common"
)

func TestServicePublishesTypedFailureAndFreshCache(test *testing.T) {
	rateLimited := false
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		if rateLimited {
			writer.WriteHeader(http.StatusTooManyRequests)
			return
		}
		fmt.Fprint(writer, `{"value":[{"id":"message","createdDateTime":"2026-09-11T11:59:00Z","messageType":"message","body":{"contentType":"text","content":"Release review"}}]}`)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	service, _ := NewService(client, "team", "channel")
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	service.clock = func() time.Time { return now }
	if _, err := service.ConnectionSnapshot(context.Background()); err != nil {
		test.Fatal(err)
	}
	rateLimited = true
	snapshot, err := service.ConnectionSnapshot(context.Background())
	value := snapshot.(common.Snapshot)
	if err != nil || value.Connection.State != "degraded" || value.Connection.LastFailure.Kind != "rate_limited" || len(value.Views) != 1 {
		test.Fatalf("snapshot: %#v %v", value, err)
	}
}
