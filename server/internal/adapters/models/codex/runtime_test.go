package codexauth

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// Synthetic credentials only; no filesystem credentials or external services.
type refreshStore struct {
	mu       sync.Mutex
	value    string
	puts     int
	putError error
	bounded  bool
}

func (s *refreshStore) Get(ctx context.Context, _ string) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.value, ctx.Err()
}
func (s *refreshStore) Put(ctx context.Context, _, value string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.puts++
	_, s.bounded = ctx.Deadline()
	if err := ctx.Err(); err != nil {
		return err
	}
	if s.putError != nil {
		return s.putError
	}
	s.value = value
	return nil
}
func (s *refreshStore) Delete(ctx context.Context, _ string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	s.value = ""
	return nil
}

func TestRefreshPersistsRotationAfterObserverCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	old := tokenBundle{AccessToken: "synthetic-old-access", RefreshToken: "synthetic-old-refresh", AccountID: "synthetic-account", ExpiresAt: time.Now().Add(-time.Minute)}
	encoded, err := json.Marshal(old)
	if err != nil {
		t.Fatal(err)
	}
	store := &refreshStore{value: string(encoded)}
	var requests atomic.Int32
	provider := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		if err := r.ParseForm(); err != nil {
			t.Error(err)
		}
		if r.Form.Get("refresh_token") != old.RefreshToken {
			t.Error("wrong synthetic refresh generation")
		}
		// The owner already dispatched rotation. Dropping its observer cannot
		// abandon custody of the provider's now-current replacement.
		cancel()
		id := "test." + base64.RawURLEncoding.EncodeToString([]byte(`{"https://api.openai.com/auth":{"account_id":"synthetic-account"}}`)) + ".test"
		_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "synthetic-new-access", "refresh_token": "synthetic-new-refresh", "id_token": id, "expires_in": 3600})
	}))
	defer provider.Close()
	runtime := New(store)
	runtime.tokenURL = provider.URL
	next, err := runtime.access(ctx)
	if err != nil {
		t.Fatalf("rotated credential custody failed after observer cancellation: %v", err)
	}
	if !errors.Is(ctx.Err(), context.Canceled) {
		t.Fatal("observer was not canceled")
	}
	if next.RefreshToken != "synthetic-new-refresh" {
		t.Fatal("wrong retained rotation")
	}
	if requests.Load() != 1 || store.puts != 1 || !store.bounded {
		t.Fatal("rotation must persist once with an owner deadline")
	}
	reopened := New(store)
	retained, err := reopened.access(context.Background())
	if err != nil || retained == nil || retained.RefreshToken != next.RefreshToken {
		t.Fatal("replacement did not survive runtime recreation")
	}
}

func TestCanceledObserverDoesNotStartCredentialRotation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	var requests atomic.Int32
	provider := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		w.WriteHeader(http.StatusServiceUnavailable)
	}))
	defer provider.Close()
	store := &refreshStore{}
	runtime := New(store)
	runtime.tokens = &tokenBundle{AccessToken: "synthetic-access", RefreshToken: "synthetic-refresh", AccountID: "synthetic-account", ExpiresAt: time.Now().Add(-time.Minute)}
	runtime.tokenURL = provider.URL
	if next, err := runtime.access(ctx); next != nil || err == nil {
		t.Fatal("canceled observer admitted")
	}
	if requests.Load() != 0 || store.puts != 0 {
		t.Fatal("canceled observer started credential work")
	}
}

func TestCredentialPersistenceFailureDoesNotPublishReplacement(t *testing.T) {
	store := &refreshStore{putError: errors.New("synthetic custody failure")}
	runtime := New(store)
	runtime.tokens = &tokenBundle{AccessToken: "synthetic-old", RefreshToken: "synthetic-old", AccountID: "synthetic-account", ExpiresAt: time.Now()}
	next := &tokenBundle{AccessToken: "synthetic-new", RefreshToken: "synthetic-new", AccountID: "synthetic-account", ExpiresAt: time.Now().Add(time.Hour)}
	if runtime.save(context.Background(), next) == nil {
		t.Fatal("custody failure hidden")
	}
	if runtime.ReplayIdentity() != "" || runtime.tokens != nil {
		t.Fatal("uncommitted replacement published")
	}
	if runtime.credentialReadFailure() == nil {
		t.Fatal("custody failure lost")
	}
}
