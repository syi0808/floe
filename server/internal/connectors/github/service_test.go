package github

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"floe/server/internal/connectors/common"
)

func TestServiceKeepsRepositorySelectionOutsideViewRequests(test *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		fmt.Fprint(writer, `[{"number":1,"title":"Selected work","body":"","state":"open","updated_at":"2026-09-10T11:59:00Z","labels":[]}]`)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	service, err := NewService(client, "acme", "floe")
	if err != nil {
		test.Fatal(err)
	}
	service.clock = func() time.Time { return time.Date(2026, 9, 10, 12, 0, 0, 0, time.UTC) }
	view, err := service.ReadWorkContextView(context.Background())
	if err != nil || view.Items[0].Title != "Selected work" {
		test.Fatalf("view: %#v %v", view, err)
	}
	snapshot, err := service.ConnectionSnapshot(context.Background())
	if err != nil || snapshot.(common.Snapshot).Connection.State != "ready" {
		test.Fatalf("snapshot: %#v %v", snapshot, err)
	}
}

func TestServicePublishesTypedFailureWithoutDroppingLastFreshView(test *testing.T) {
	status := http.StatusOK
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.WriteHeader(status)
		if status == http.StatusOK {
			fmt.Fprint(writer, `[{"number":1,"title":"Selected work","body":"","state":"open","updated_at":"2026-09-10T11:59:00Z","labels":[]}]`)
		}
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	service, _ := NewService(client, "acme", "floe")
	now := time.Date(2026, 9, 10, 12, 0, 0, 0, time.UTC)
	service.clock = func() time.Time { return now }
	if _, err := service.ConnectionSnapshot(context.Background()); err != nil {
		test.Fatal(err)
	}
	status = http.StatusTooManyRequests
	snapshot, err := service.ConnectionSnapshot(context.Background())
	value := snapshot.(common.Snapshot)
	if err != nil || value.Connection.State != "degraded" || value.Connection.LastFailure.Kind != "rate_limited" || len(value.Views) != 1 {
		test.Fatalf("snapshot: %#v %v", value, err)
	}
}
