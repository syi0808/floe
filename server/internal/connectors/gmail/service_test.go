package gmail

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"time"
)

type serviceAuth struct {
	ready      bool
	actions    []string
	tokenError error
	actionErr  error
}

func (auth *serviceAuth) Ready() bool { return auth.ready }
func (auth *serviceAuth) Token(context.Context) (string, error) {
	if auth.tokenError != nil {
		auth.ready = false
		return "", auth.tokenError
	}
	return "secret", nil
}
func (auth *serviceAuth) Action(_ context.Context, action string) (any, error) {
	auth.actions = append(auth.actions, action)
	if action == "logout" {
		auth.ready = false
	}
	if auth.actionErr != nil {
		return nil, auth.actionErr
	}
	return map[string]any{"status": map[bool]string{true: "connected", false: "disconnected"}[auth.ready]}, nil
}

func TestServicePersistsLifecycleDegradesAndDisconnectClearsViews(t *testing.T) {
	status := http.StatusOK
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if status != http.StatusOK {
			writer.WriteHeader(status)
			return
		}
		switch request.URL.Path {
		case "/users/me/profile":
			fmt.Fprint(writer, `{"historyId":"40"}`)
		case "/users/me/messages":
			fmt.Fprint(writer, `{"messages":[{"id":"m1","threadId":"t1"}]}`)
		case "/users/me/messages/m1":
			fmt.Fprint(writer, metadataJSON("m1", "t1", "Reply needed", "40"))
		case "/users/me/history":
			fmt.Fprint(writer, `{"historyId":"41"}`)
		default:
			t.Fatalf("unexpected request: %s", request.URL.Path)
		}
	}))
	defer server.Close()
	directory := t.TempDir()
	os.Chmod(directory, 0700)
	auth := &serviceAuth{ready: true}
	client, _ := NewWithBaseURL(auth, server.URL)
	service, err := newService(directory, "account-1", "newer_than:30d", auth, client)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Unix(1_789_000_000, 0)
	service.clock = func() time.Time { return now }
	pending, _ := service.Action(context.Background(), "status")
	if pending.(map[string]any)["connection"].(Snapshot).Connection.State != "pending" {
		t.Fatalf("pending: %#v", pending)
	}
	value, err := service.Action(context.Background(), "sync")
	if err != nil {
		t.Fatal(err)
	}
	ready := value.(Snapshot)
	if ready.Connection.State != "ready" || ready.Connection.LastSuccessAtUnixMS == nil || len(ready.Views) != 1 {
		t.Fatalf("ready: %#v", ready)
	}
	viewValue, err := service.ReadCommunicationView("reply", 0, 25)
	if err != nil {
		t.Fatal(err)
	}
	view := viewValue.(CommunicationView)
	if len(view.Items) != 1 || view.Items[0].Subject != "Reply needed" {
		t.Fatalf("view: %#v", view)
	}

	reopened, err := newService(directory, "account-1", "newer_than:30d", auth, client)
	if err != nil {
		t.Fatal(err)
	}
	reopened.clock = service.clock
	restored, _ := reopened.Action(context.Background(), "status")
	if restored.(map[string]any)["connection"].(Snapshot).Connection.State != "ready" {
		t.Fatalf("restored: %#v", restored)
	}
	status = http.StatusTooManyRequests
	if _, err := reopened.Action(context.Background(), "sync"); !errors.Is(err, ErrRateLimited) {
		t.Fatalf("sync: %v", err)
	}
	degraded, _ := reopened.Action(context.Background(), "status")
	degradedSnapshot := degraded.(map[string]any)["connection"].(Snapshot)
	if degradedSnapshot.Connection.State != "degraded" || degradedSnapshot.Connection.LastFailure.Kind != "rate_limited" || len(degradedSnapshot.Views) != 1 {
		t.Fatalf("degraded: %#v", degraded)
	}
	status = http.StatusOK
	auth.tokenError = ErrCredentialExpired
	if _, err := reopened.Action(context.Background(), "sync"); !errors.Is(err, ErrCredentialExpired) {
		t.Fatalf("expired credential: %v", err)
	}
	revoked, _ := reopened.Action(context.Background(), "status")
	revokedSnapshot := revoked.(map[string]any)["connection"].(Snapshot)
	if revokedSnapshot.Connection.State != "revoked" || len(revokedSnapshot.Views) != 0 {
		t.Fatalf("revoked: %#v", revoked)
	}
	if _, err := reopened.ReadCommunicationView("", 0, 25); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("revoked view read: %v", err)
	}
	auth.tokenError = nil
	auth.actionErr = ErrUnavailable
	if _, err := reopened.Action(context.Background(), "logout"); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("logout error: %v", err)
	}
	auth.actionErr = nil
	disconnected, _ := reopened.Action(context.Background(), "status")
	disconnectedSnapshot := disconnected.(map[string]any)["connection"].(Snapshot)
	if disconnectedSnapshot.Connection.State != "disconnected" || len(disconnectedSnapshot.Views) != 0 || reopened.index.HistoryID() != "" {
		t.Fatalf("disconnected: %#v", disconnected)
	}
}

func TestServiceRejectsConcurrentInvalidAndOutOfRangeScheduling(t *testing.T) {
	directory := t.TempDir()
	os.Chmod(directory, 0700)
	auth := &serviceAuth{}
	client, _ := NewWithBaseURL(auth, "http://127.0.0.1:1")
	service, _ := newService(directory, "account-1", "newer_than:30d", auth, client)
	if _, err := service.Action(context.Background(), "unknown"); !errors.Is(err, ErrInvalidInput) {
		t.Fatal(err)
	}
	if _, err := service.Action(context.Background(), "sync"); !errors.Is(err, ErrCredentialExpired) {
		t.Fatal(err)
	}
	if err := service.Run(context.Background(), time.Second); !errors.Is(err, ErrInvalidInput) {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := service.Run(ctx, time.Minute); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}
