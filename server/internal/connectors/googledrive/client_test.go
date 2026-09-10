package googledrive

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

func TestSelectedFolderProjectsBoundedTextFiles(test *testing.T) {
	now := time.Date(2026, 9, 11, 12, 0, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer private-token" || request.Method != http.MethodGet {
			test.Fatal("missing bounded read authentication")
		}
		switch request.URL.Path {
		case "/drive/v3/files":
			if request.URL.Query().Get("q") != "'folder12345' in parents and trashed = false" || request.URL.Query().Get("pageSize") != "8" {
				test.Fatalf("unbounded query: %s", request.URL.RawQuery)
			}
			fmt.Fprint(writer, `{"files":[{"id":"document123","name":"Launch notes","mimeType":"application/vnd.google-apps.document","modifiedTime":"2026-09-11T11:59:00Z"},{"id":"binary12345","name":"Design.png","mimeType":"image/png","modifiedTime":"2026-09-11T11:58:00Z"}],"nextPageToken":""}`)
		case "/drive/v3/files/document123/export":
			if request.URL.Query().Get("mimeType") != "text/plain" {
				test.Fatalf("unsafe export: %s", request.URL.RawQuery)
			}
			fmt.Fprint(writer, "Release readiness\nAttach validation evidence.")
		default:
			test.Fatalf("unexpected request: %s", request.URL.RequestURI())
		}
	}))
	defer server.Close()
	client, err := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	if err != nil {
		test.Fatal(err)
	}
	view, err := client.WorkContext(context.Background(), "folder12345", now)
	if err != nil {
		test.Fatal(err)
	}
	if len(view.Items) != 1 || view.Items[0].Kind != "selected_file" || view.Items[0].Title != "Launch notes" || *view.Items[0].Excerpt != "Release readiness\nAttach validation evidence." {
		test.Fatalf("view: %#v", view)
	}
	encoded := fmt.Sprintf("%#v", view)
	for _, private := range []string{"private-token", "folder12345", "document123", "application/vnd.google-apps.document"} {
		if strings.Contains(encoded, private) {
			test.Fatalf("provider detail leaked: %s", private)
		}
	}
}

func TestDriveFailuresAndUnsafeSelectionAreRejected(test *testing.T) {
	status := http.StatusUnauthorized
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.WriteHeader(status)
	}))
	defer server.Close()
	client, _ := NewWithBaseURL(tokenSource{token: "private-token"}, server.URL)
	if _, err := client.WorkContext(context.Background(), "folder12345", time.Now()); !errors.Is(err, ErrCredentialExpired) {
		test.Fatal(err)
	}
	status = http.StatusForbidden
	if _, err := client.WorkContext(context.Background(), "folder12345", time.Now()); !errors.Is(err, ErrPermissionDenied) {
		test.Fatal(err)
	}
	status = http.StatusTooManyRequests
	if _, err := client.WorkContext(context.Background(), "folder12345", time.Now()); !errors.Is(err, ErrRateLimited) {
		test.Fatal(err)
	}
	if _, err := client.WorkContext(context.Background(), "../all", time.Now()); !errors.Is(err, ErrInvalidInput) {
		test.Fatal(err)
	}
	for _, endpoint := range []string{"http://www.googleapis.com", "https://user@www.googleapis.com", "https://www.googleapis.com/drive"} {
		if _, err := NewWithBaseURL(tokenSource{token: "private-token"}, endpoint); !errors.Is(err, ErrInvalidInput) {
			test.Fatalf("unsafe endpoint accepted: %s", endpoint)
		}
	}
}
