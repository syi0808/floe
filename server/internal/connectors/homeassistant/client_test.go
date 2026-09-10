package homeassistant

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

type tokenSource struct{ token string }

func (source tokenSource) Token(context.Context) (string, error) { return source.token, nil }

func TestAllowlistedEntitiesProjectOnlyBoundedHomeState(test *testing.T) {
	now := time.Date(2026, 9, 10, 12, 0, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.URL.Path != "/api/states/sensor.front_door_temperature" {
			test.Fatalf("request: %s %s", request.Method, request.URL.Path)
		}
		if request.Header.Get("Authorization") != "Bearer private-token" {
			test.Fatal("missing bearer token")
		}
		fmt.Fprint(writer, `{"entity_id":"sensor.front_door_temperature","state":"22.5","last_updated":"2026-09-10T11:59:00Z","attributes":{"friendly_name":"Front door temperature","unit_of_measurement":"°C","private":"ignored"},"context":{"id":"provider-id"}}`)
	}))
	defer server.Close()
	client, err := New(tokenSource{token: "private-token"}, server.URL, "home-1")
	if err != nil {
		test.Fatal(err)
	}
	view, err := client.Logistics(context.Background(), []string{"sensor.front_door_temperature"}, now)
	if err != nil {
		test.Fatal(err)
	}
	if len(view.Items) != 1 || view.Items[0].Summary != "Front door temperature" || view.Items[0].Status != "22.5" {
		test.Fatalf("view: %#v", view)
	}
	snapshot, err := ConnectionSnapshot(view)
	if err != nil || snapshot.Connection.State != "ready" || snapshot.Views[0].ProvenanceCount != 1 {
		test.Fatalf("snapshot: %#v %v", snapshot, err)
	}
	encoded := fmt.Sprintf("%#v", view)
	for _, private := range []string{"private-token", "sensor.front_door_temperature", "provider-id", "unit_of_measurement"} {
		if strings.Contains(encoded, private) {
			test.Fatalf("raw provider detail leaked: %s", private)
		}
	}
}

func TestSecurityEntitiesUnsafeEndpointsAndProviderFailuresAreRejected(test *testing.T) {
	for _, endpoint := range []string{"http://example.com", "ftp://127.0.0.1", "https://user@example.com", "https://example.com/hidden"} {
		if _, err := New(tokenSource{token: "token-value"}, endpoint, "home-1"); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe endpoint accepted: %s", endpoint)
		}
	}
	client, _ := New(tokenSource{token: "token-value"}, "http://127.0.0.1:1", "home-1")
	for _, entity := range []string{"lock.front_door", "alarm_control_panel.home", "../sensor.private", "sensor.UPPER"} {
		if _, err := client.Logistics(context.Background(), []string{entity}, time.Now()); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe entity accepted: %s", entity)
		}
	}

	status := http.StatusUnauthorized
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.WriteHeader(status)
	}))
	defer server.Close()
	client, _ = New(tokenSource{token: "token-value"}, server.URL, "home-1")
	if _, err := client.Logistics(context.Background(), []string{"sensor.temperature"}, time.Now()); !errors.Is(err, ErrCredentialExpired) {
		test.Fatal(err)
	}
	status = http.StatusTooManyRequests
	if _, err := client.Logistics(context.Background(), []string{"sensor.temperature"}, time.Now()); !errors.Is(err, ErrRateLimited) {
		test.Fatal(err)
	}
}
