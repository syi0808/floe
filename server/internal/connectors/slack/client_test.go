package slack

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

func TestSelectedConversationProjectsOnlyBoundedWorkContext(test *testing.T) {
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.URL.RequestURI() != "/conversations.history?channel=C12345678&limit=50" {
			test.Fatalf("request: %s %s", request.Method, request.URL.RequestURI())
		}
		if request.Header.Get("Authorization") != "Bearer private-token" {
			test.Fatal("missing bearer token")
		}
		fmt.Fprint(writer, `{"ok":true,"messages":[{"type":"message","user":"U12345678","text":"Release review\nAttach the validation evidence.","ts":"1789127940.123456","files":[{"url_private":"private"}]},{"type":"message","subtype":"channel_join","text":"ignored","ts":"1789127930.000000"}],"response_metadata":{"next_cursor":""}}`)
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	if err != nil {
		test.Fatal(err)
	}
	view, err := client.WorkContext(context.Background(), "C12345678", "", now)
	if err != nil {
		test.Fatal(err)
	}
	if len(view.Items) != 1 || view.Items[0].Kind != "communication" || view.Items[0].Title != "Release review" || view.Items[0].ObservedAtUnixMS != 1_789_127_940_123 {
		test.Fatalf("view: %#v", view)
	}
	encoded := fmt.Sprintf("%#v", view)
	for _, private := range []string{"private-token", "C12345678", "U12345678", "url_private"} {
		if strings.Contains(encoded, private) {
			test.Fatalf("provider detail leaked: %s", private)
		}
	}
}

func TestThreadSelectionAndProviderFailuresAreTyped(test *testing.T) {
	providerErrorValue := "invalid_auth"
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.RequestURI() != "/conversations.replies?channel=G12345678&limit=50&ts=1789127940.123456" {
			test.Fatalf("request: %s", request.URL.RequestURI())
		}
		fmt.Fprintf(writer, `{"ok":false,"error":%q}`, providerErrorValue)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	if _, err := client.WorkContext(context.Background(), "G12345678", "1789127940.123456", time.Now()); !errors.Is(err, ErrCredentialExpired) {
		test.Fatal(err)
	}
	providerErrorValue = "missing_scope"
	if _, err := client.WorkContext(context.Background(), "G12345678", "1789127940.123456", time.Now()); !errors.Is(err, ErrPermissionDenied) {
		test.Fatal(err)
	}
}

func TestUnsafeSlackScopesAndEndpointsAreRejected(test *testing.T) {
	for _, endpoint := range []string{"http://slack.com/api", "https://user@slack.com/api", "https://slack.com/other"} {
		if _, err := NewWithBaseURL(tokenSource{token: "private-token"}, endpoint); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe endpoint accepted: %s", endpoint)
		}
	}
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, "http://127.0.0.1:1")
	for _, selection := range [][2]string{{"*", ""}, {"D12345678", ""}, {"C12345678", "latest"}} {
		if _, err := client.WorkContext(context.Background(), selection[0], selection[1], time.Now()); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe selection accepted: %#v", selection)
		}
	}
}
