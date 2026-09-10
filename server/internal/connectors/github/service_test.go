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
	if err != nil || view.(WorkContextView).Items[0].Title != "Selected work" {
		test.Fatalf("view: %#v %v", view, err)
	}
	snapshot, err := service.ConnectionSnapshot(context.Background())
	if err != nil || snapshot.(common.Snapshot).Connection.State != "ready" {
		test.Fatalf("snapshot: %#v %v", snapshot, err)
	}
}
