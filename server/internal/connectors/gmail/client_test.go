package gmail

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

type tokenSource string

func (source tokenSource) Token(context.Context) (string, error) { return string(source), nil }

type bodyAuthority struct{ messageID string }

func (authority bodyAuthority) AuthorizeBodyRead(_ context.Context, messageID string) error {
	if messageID != authority.messageID {
		return ErrBodyApproval
	}
	return nil
}

func TestSearchMetadataBodyAndChangesAreBoundedReadOnly(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.Header.Get("Authorization") != "Bearer secret" {
			t.Fatalf("unexpected request authority: %s %q", request.Method, request.Header.Get("Authorization"))
		}
		switch {
		case request.URL.Path == "/users/me/messages" && request.URL.Query().Get("q") == "is:unread":
			writer.Write([]byte(`{"messages":[{"id":"m1","threadId":"t1"}],"nextPageToken":"next"}`))
		case request.URL.Path == "/users/me/messages/m1" && request.URL.Query().Get("format") == "metadata":
			writer.Write([]byte(`{"id":"m1","threadId":"t1","historyId":"42","internalDate":"1789000000000","labelIds":["INBOX"],"snippet":"Need a reply","payload":{"headers":[{"name":"From","value":"person@example.com"},{"name":"Subject","value":"Launch approval"}]}}`))
		case request.URL.Path == "/users/me/messages/m1" && request.URL.Query().Get("format") == "full":
			body := base64.RawURLEncoding.EncodeToString([]byte("Please confirm by Friday."))
			writer.Write([]byte(`{"payload":{"mimeType":"multipart/alternative","parts":[{"mimeType":"text/plain","body":{"data":"` + body + `"}}]}}`))
		case request.URL.Path == "/users/me/history":
			if request.URL.Query().Get("startHistoryId") != "40" {
				t.Fatal("missing checkpoint")
			}
			writer.Write([]byte(`{"history":[{"messagesAdded":[{"message":{"id":"m2","threadId":"t2"}}],"messagesDeleted":[{"message":{"id":"m3","threadId":"t3"}}]}],"historyId":"43"}`))
		default:
			t.Fatalf("unexpected path: %s?%s", request.URL.Path, request.URL.RawQuery)
		}
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource("secret"), server.URL)
	if err != nil {
		t.Fatal(err)
	}

	page, err := client.Search(context.Background(), "is:unread", "", 10)
	if err != nil || len(page.Messages) != 1 || page.NextCursor != "next" {
		t.Fatalf("search: %#v %v", page, err)
	}
	metadata, err := client.ReadMetadata(context.Background(), "m1")
	if err != nil || metadata.Subject != "Launch approval" || metadata.Snippet != "Need a reply" {
		t.Fatalf("metadata: %#v %v", metadata, err)
	}
	if _, err := client.ReadBody(context.Background(), "m1", nil); !errors.Is(err, ErrBodyApproval) {
		t.Fatalf("body without approval: %v", err)
	}
	body, err := client.ReadBody(context.Background(), "m1", bodyAuthority{messageID: "m1"})
	if err != nil || body != "Please confirm by Friday." {
		t.Fatalf("body: %q %v", body, err)
	}
	changes, err := client.Changes(context.Background(), "40", "", 10)
	if err != nil || changes.HistoryID != "43" || len(changes.Added) != 1 || len(changes.Deleted) != 1 {
		t.Fatalf("changes: %#v %v", changes, err)
	}
}

func TestCredentialRateLimitCheckpointAndEnvelopeFailuresAreTyped(t *testing.T) {
	statuses := []struct {
		path   string
		status int
		want   error
	}{
		{"/unauthorized", http.StatusUnauthorized, ErrCredentialExpired},
		{"/limited", http.StatusTooManyRequests, ErrRateLimited},
	}
	for _, test := range statuses {
		t.Run(test.path, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) { writer.WriteHeader(test.status) }))
			defer server.Close()
			client, _ := NewWithBaseURL(tokenSource("secret"), server.URL+test.path)
			_, err := client.Search(context.Background(), "x", "", 1)
			if !errors.Is(err, test.want) {
				t.Fatalf("got %v", err)
			}
		})
	}

	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if strings.Contains(request.URL.Path, "/history") {
			writer.WriteHeader(http.StatusNotFound)
			return
		}
		writer.Write([]byte(`{"messages":[{"id":"contains/slash","threadId":"t"}]}`))
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource("secret"), server.URL)
	if _, err := client.Search(context.Background(), "x", "", 1); !errors.Is(err, ErrInvalidResponse) {
		t.Fatalf("invalid envelope: %v", err)
	}
	if _, err := client.Changes(context.Background(), "old", "", 1); !errors.Is(err, ErrCheckpointExpired) {
		t.Fatalf("checkpoint: %v", err)
	}
}

func TestRejectsUntrustedEndpointsAndInputsBeforeNetwork(t *testing.T) {
	for _, endpoint := range []string{"http://example.com", "https://user@example.com", "https://example.com?token=x"} {
		if _, err := NewWithBaseURL(tokenSource("secret"), endpoint); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted %q: %v", endpoint, err)
		}
	}
	client, _ := NewWithBaseURL(tokenSource("secret"), "http://127.0.0.1:1")
	if _, err := client.Search(context.Background(), "", "", 1); !errors.Is(err, ErrInvalidInput) {
		t.Fatal(err)
	}
	if _, err := client.ReadMetadata(context.Background(), "../token"); !errors.Is(err, ErrInvalidInput) {
		t.Fatal(err)
	}
	if _, err := client.Changes(context.Background(), "", "", 1); !errors.Is(err, ErrInvalidInput) {
		t.Fatal(err)
	}
}

func TestDescriptorSeparatesObserveFromFutureActionsAndRedactsNativeIDs(t *testing.T) {
	descriptor := ConnectorDescriptor()
	if len(descriptor.Capabilities) != 4 || len(descriptor.Views) != 2 {
		t.Fatalf("unexpected descriptor: %#v", descriptor)
	}
	for _, capability := range descriptor.Capabilities {
		if capability.Authority != "observe" || len(capability.RequiredScopes) != 1 {
			t.Fatalf("unexpected authority: %#v", capability)
		}
	}
	handle, err := SourceHandle("account-1", "native-message-1")
	if err != nil || strings.Contains(handle, "account-1") || strings.Contains(handle, "native-message-1") {
		t.Fatalf("unsafe handle: %q %v", handle, err)
	}
	now := time.Unix(1_789_000_000, 0)
	snapshot, err := ConnectionSnapshot("account-1", "degraded", now, &now, "rate_limited")
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(snapshot)
	if err != nil || strings.Contains(string(encoded), "account-1") {
		t.Fatalf("snapshot leaked native identity: %s %v", encoded, err)
	}
}
