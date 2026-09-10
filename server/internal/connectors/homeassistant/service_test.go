package homeassistant

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestServiceKeepsEntityAllowlistOutsideViewRequests(test *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		fmt.Fprint(writer, `{"entity_id":"sensor.temperature","state":"22","last_updated":"2026-09-10T11:59:00Z","attributes":{"friendly_name":"Temperature"}}`)
	}))
	defer server.Close()
	client, _ := New(tokenSource{token: "private-token"}, server.URL, "home-1")
	service, err := NewService(client, []string{"sensor.temperature"})
	if err != nil {
		test.Fatal(err)
	}
	service.clock = func() time.Time { return time.Date(2026, 9, 10, 12, 0, 0, 0, time.UTC) }
	view, err := service.ReadLogisticsView(context.Background())
	if err != nil || view.(LogisticsView).Items[0].Summary != "Temperature" {
		test.Fatalf("view: %#v %v", view, err)
	}
	if _, err := service.ConnectionSnapshot(context.Background()); err != nil {
		test.Fatal(err)
	}
}
