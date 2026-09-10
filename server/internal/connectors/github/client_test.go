package github

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

type tokenSource struct {
	token string
	err   error
}

func (source tokenSource) Token(context.Context) (string, error) {
	return source.token, source.err
}

func TestSelectedRepositoryProjectsToBoundedWorkContext(test *testing.T) {
	now := time.Date(2026, 9, 10, 12, 0, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.URL.RequestURI() != "/repos/acme/floe/issues?state=open&per_page=50" {
			test.Fatalf("request: %s %s", request.Method, request.URL.RequestURI())
		}
		if request.Header.Get("Authorization") != "Bearer private-token" || request.Header.Get("X-GitHub-Api-Version") != "2022-11-28" {
			test.Fatal("missing bounded GitHub authentication headers")
		}
		fmt.Fprint(writer, `[
          {"number":42,"title":"Release readiness","body":"Attach API evidence.","state":"open","updated_at":"2026-09-10T11:59:00Z","labels":[{"name":"blocked"}]},
          {"number":43,"title":"A pull request","body":"ignored","state":"open","updated_at":"2026-09-10T11:59:00Z","labels":[],"pull_request":{}}
        ]`)
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	if err != nil {
		test.Fatal(err)
	}
	view, err := client.WorkContext(context.Background(), "acme", "floe", now)
	if err != nil {
		test.Fatal(err)
	}
	if len(view.Items) != 1 || view.Items[0].Title != "Release readiness" || view.Items[0].Blocker == nil {
		test.Fatalf("view: %#v", view)
	}
	snapshot, err := ConnectionSnapshot(view)
	if err != nil || snapshot.Connection.State != "ready" || len(snapshot.Views) != 1 || snapshot.Views[0].ProvenanceCount != 1 {
		test.Fatalf("snapshot: %#v %v", snapshot, err)
	}
	encoded := fmt.Sprintf("%#v", view)
	for _, private := range []string{"private-token", "https://github.com", "#42"} {
		if strings.Contains(encoded, private) {
			test.Fatalf("provider detail leaked: %s", private)
		}
	}
}

func TestClientRejectsUnsafeEndpointsCredentialsAndProviderFailures(test *testing.T) {
	for _, endpoint := range []string{"http://example.com", "https://user@example.com", "https://example.com/path"} {
		if _, err := NewWithBaseURL(tokenSource{token: "token-value"}, endpoint); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe endpoint accepted: %s", endpoint)
		}
	}
	if _, err := New(nil); !errors.Is(err, ErrInvalidInput) {
		test.Fatal(err)
	}

	status := http.StatusUnauthorized
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.WriteHeader(status)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "token-value"}, server.URL)
	if _, err := client.WorkContext(context.Background(), "acme", "floe", time.Now()); !errors.Is(err, ErrCredentialExpired) {
		test.Fatal(err)
	}
	status = http.StatusTooManyRequests
	if _, err := client.WorkContext(context.Background(), "acme", "floe", time.Now()); !errors.Is(err, ErrRateLimited) {
		test.Fatal(err)
	}
	if _, err := client.WorkContext(context.Background(), "../owner", "repo", time.Now()); !errors.Is(err, ErrInvalidInput) {
		test.Fatal(err)
	}
}
