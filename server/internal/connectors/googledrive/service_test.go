package googledrive

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"floe/server/internal/connectors/common"
)

func TestServicePublishesTypedCredentialFailure(test *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.WriteHeader(http.StatusUnauthorized)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	service, _ := NewService(client, "folder12345")
	snapshot, err := service.ConnectionSnapshot(context.Background())
	value := snapshot.(common.Snapshot)
	if err != nil || value.Connection.State != "revoked" || value.Connection.LastFailure.Kind != "credential_expired" {
		test.Fatalf("snapshot: %#v %v", value, err)
	}
}
