package microsoftmail

import (
	"context"
	"encoding/json"
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

func (source tokenSource) Token(context.Context) (string, error) { return source.token, source.err }

func TestCommunicationReadsOnlyBoundedSelectedInboxMetadata(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.URL.Path != "/me/mailFolders/inbox/messages" {
			t.Fatalf("unexpected request: %s %s", request.Method, request.URL.Path)
		}
		if request.Header.Get("Authorization") != "Bearer private-token" || request.Header.Get("ConsistencyLevel") != "eventual" {
			t.Fatalf("unexpected headers: %#v", request.Header)
		}
		query := request.URL.Query()
		if query.Get("$top") != "25" || query.Get("$skip") != "0" || query.Get("$search") != `"launch \"review\""` || query.Has("$orderby") {
			t.Fatalf("unexpected query: %s", request.URL.RawQuery)
		}
		fmt.Fprint(writer, `{"value":[{"id":"message_123","conversationId":"conversation_123","receivedDateTime":"2026-09-11T11:59:00Z","subject":"Confirm review","bodyPreview":"Please confirm by Friday.","from":{"emailAddress":{"address":"alex@example.com"}},"toRecipients":[{"emailAddress":{"address":"person@example.com"}}],"categories":["Focused"],"internetMessageHeaders":[{"name":"private","value":"ignored"}]}],"@odata.nextLink":""}`)
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "account-1")
	if err != nil {
		t.Fatal(err)
	}
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	view, err := client.Communication(context.Background(), `launch "review"`, 0, 25, now)
	if err != nil || len(view.Items) != 1 || view.Items[0].Subject != "Confirm review" || view.Items[0].Snippet != "Please confirm by Friday." || !view.CoverageComplete || view.NextCursor != nil {
		t.Fatalf("view: %#v %v", view, err)
	}
	encoded, err := json.Marshal(view)
	if err != nil || strings.Contains(string(encoded), "message_123") || strings.Contains(string(encoded), "conversation_123") || strings.Contains(string(encoded), "ignored") || strings.Contains(string(encoded), "private-token") {
		t.Fatalf("view leaked provider data: %s %v", encoded, err)
	}
}

func TestCommunicationPublishesBoundedNextCursor(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Query().Get("$orderby") != "receivedDateTime desc" || request.Header.Get("ConsistencyLevel") != "" {
			t.Fatalf("unexpected unfiltered request: %s %#v", request.URL.RawQuery, request.Header)
		}
		fmt.Fprint(writer, `{"value":[{"id":"id:with?opaque#characters","conversationId":"thread:opaque","receivedDateTime":"2026-09-11T11:59:00Z","categories":[]}],"@odata.nextLink":"https://graph.microsoft.com/v1.0/opaque"}`)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "account-1")
	view, err := client.Communication(context.Background(), "", 40, 10, time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC))
	if err != nil || view.CoverageComplete || view.NextCursor == nil || *view.NextCursor != 41 {
		t.Fatalf("view: %#v %v", view, err)
	}
}

func TestCommunicationFailuresAreTyped(t *testing.T) {
	for _, test := range []struct {
		status int
		want   error
	}{{http.StatusUnauthorized, ErrCredentialExpired}, {http.StatusForbidden, ErrPermissionDenied}, {http.StatusTooManyRequests, ErrRateLimited}, {http.StatusBadGateway, ErrUnavailable}} {
		t.Run(fmt.Sprint(test.status), func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) { writer.WriteHeader(test.status) }))
			defer server.Close()
			client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "account-1")
			_, err := client.Communication(context.Background(), "", 0, 1, time.Now())
			if !errors.Is(err, test.want) {
				t.Fatalf("got %v", err)
			}
		})
	}
	client, _ := NewWithBaseURL(tokenSource{err: errors.New("expired")}, "http://127.0.0.1:1", "account-1")
	if _, err := client.Communication(context.Background(), "", 0, 1, time.Now()); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("credential error: %v", err)
	}
}

func TestRejectsUntrustedEndpointsInputsAndResponses(t *testing.T) {
	for _, endpoint := range []string{"http://example.com/v1.0", "https://user@example.com/v1.0", "https://example.com/v1.0?token=x", "https://example.com/not-graph"} {
		if _, err := NewWithBaseURL(tokenSource{token: "private-token"}, endpoint, "account-1"); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted %q: %v", endpoint, err)
		}
	}
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, "http://127.0.0.1:1", "account-1")
	for _, input := range []struct {
		query  string
		cursor int
		limit  int
	}{{"bad\nquery", 0, 1}, {"", -1, 1}, {"", 0, 0}, {"", 0, 101}} {
		if _, err := client.Communication(context.Background(), input.query, input.cursor, input.limit, time.Now()); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted input: %#v %v", input, err)
		}
	}
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		fmt.Fprint(writer, `{"value":[{"id":"message","conversationId":"thread","receivedDateTime":"not-a-time"}]}`)
	}))
	defer server.Close()
	invalid, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL, "account-1")
	if _, err := invalid.Communication(context.Background(), "", 0, 1, time.Now()); !errors.Is(err, ErrInvalidResponse) {
		t.Fatalf("invalid response: %v", err)
	}
}
