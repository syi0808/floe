package microsoftmail

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
		fmt.Fprint(writer, `{"value":[{"id":"message_123","conversationId":"conversation_123","receivedDateTime":"2026-09-11T11:59:00Z","subject":"Confirm review","categories":[]}]}`)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "account-1")
	service, _ := NewService(client)
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	service.clock = func() time.Time { return now }
	if _, err := service.ConnectionSnapshot(context.Background()); err != nil {
		t.Fatal(err)
	}
	rateLimited = true
	snapshot, err := service.ConnectionSnapshot(context.Background())
	value := snapshot.(common.Snapshot)
	if err != nil || value.Connection.State != "degraded" || value.Connection.LastFailure.Kind != "rate_limited" || len(value.Views) != 1 || value.Connection.LastSuccessAtUnixMS == nil {
		t.Fatalf("snapshot: %#v %v", value, err)
	}
}

func TestServicePublishesRevokedCredentialWithoutCache(t *testing.T) {
	client, _ := NewWithBaseURL(tokenSource{err: fmt.Errorf("expired")}, "http://127.0.0.1:1", "account-1")
	service, _ := NewService(client)
	snapshot, err := service.ConnectionSnapshot(context.Background())
	value := snapshot.(common.Snapshot)
	if err != nil || value.Connection.State != "revoked" || value.Connection.LastFailure.Kind != "credential_expired" || len(value.Views) != 0 {
		t.Fatalf("snapshot: %#v %v", value, err)
	}
}
