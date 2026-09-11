package microsoftteams

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

func TestSelectedChannelProjectsOnlyBoundedWorkContext(test *testing.T) {
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet || request.URL.Path != "/teams/2f5d86d0-2527-4c94-8f03-33423b9db904/channels/19:launch@thread.tacv2/messages" || request.URL.Query().Get("$top") != "50" {
			test.Fatalf("request: %s %s", request.Method, request.URL.RequestURI())
		}
		if request.Header.Get("Authorization") != "Bearer private-token" {
			test.Fatal("missing bearer token")
		}
		fmt.Fprint(writer, `{"@odata.nextLink":"","value":[{"id":"1789127940123","createdDateTime":"2026-09-11T11:59:00Z","lastModifiedDateTime":"2026-09-11T11:59:30Z","messageType":"message","body":{"contentType":"html","content":"<p>Release review<br>Attach the validation evidence.</p><script>secret()</script>"},"from":{"user":{"id":"private-user"}},"attachments":[{"contentUrl":"https://private"}]},{"id":"deleted","createdDateTime":"2026-09-11T11:58:00Z","messageType":"message","deletedDateTime":"2026-09-11T11:59:00Z","body":{"contentType":"text","content":"ignored"}}]}`)
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	if err != nil {
		test.Fatal(err)
	}
	view, err := client.WorkContext(context.Background(), "2f5d86d0-2527-4c94-8f03-33423b9db904", "19:launch@thread.tacv2", now)
	if err != nil {
		test.Fatal(err)
	}
	if len(view.Items) != 1 || view.Items[0].Kind != "communication" || view.Items[0].Title != "Release review" || view.Items[0].ObservedAtUnixMS != 1_789_127_970_000 {
		test.Fatalf("view: %#v", view)
	}
	encoded := fmt.Sprintf("%#v", view)
	for _, private := range []string{"private-token", "private-user", "2f5d86d0", "19:launch", "contentUrl", "secret"} {
		if strings.Contains(encoded, private) {
			test.Fatalf("provider detail leaked: %s", private)
		}
	}
}

func TestProviderFailuresAndUnsafeScopeFailClosed(test *testing.T) {
	status := http.StatusUnauthorized
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(status)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	if _, err := client.WorkContext(context.Background(), "team", "channel", time.Now()); !errors.Is(err, ErrCredentialExpired) {
		test.Fatal(err)
	}
	status = http.StatusForbidden
	if _, err := client.WorkContext(context.Background(), "team", "channel", time.Now()); !errors.Is(err, ErrPermissionDenied) {
		test.Fatal(err)
	}
	status = http.StatusTooManyRequests
	if _, err := client.WorkContext(context.Background(), "team", "channel", time.Now()); !errors.Is(err, ErrRateLimited) {
		test.Fatal(err)
	}
	for _, selection := range [][2]string{{"*", "channel"}, {"team", "../messages"}, {"team?all=true", "channel"}} {
		if _, err := client.WorkContext(context.Background(), selection[0], selection[1], time.Now()); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe selection accepted: %#v", selection)
		}
	}
	for _, endpoint := range []string{"http://graph.microsoft.com/v1.0", "https://user@graph.microsoft.com/v1.0", "https://graph.microsoft.com/beta"} {
		if _, err := NewWithBaseURL(tokenSource{token: "private-token"}, endpoint); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe endpoint accepted: %s", endpoint)
		}
	}
}

func TestMessageBodyRequiresKnownTypeAndRemovesMarkup(test *testing.T) {
	text, err := messageText("html", `<div>Hello &amp; welcome<br/>Next</div><style>.private{}</style>`)
	if err != nil || strings.Contains(text, "private") || text != "Hello & welcome\nNext" {
		test.Fatalf("text=%q err=%v", text, err)
	}
	if _, err := messageText("markdown", "private"); !errors.Is(err, ErrInvalidResponse) {
		test.Fatal("unknown content type accepted")
	}
}
