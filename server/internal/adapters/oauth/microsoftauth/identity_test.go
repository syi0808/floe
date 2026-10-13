package microsoftauth

import (
	"context"
	"crypto"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"math/big"
	"net/http"
	"net/http/httptest"
	"net/url"
	"sync"
	"testing"
	"time"

	"floe/server/internal/integrations"
	"floe/server/internal/trust"
)

// All credential values in this fixture are synthetic and stay in memory.
type identityStore struct {
	mu     sync.Mutex
	values map[string]string
}

func (s *identityStore) Get(ctx context.Context, key string) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.values[key], ctx.Err()
}
func (s *identityStore) Put(ctx context.Context, key, value string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	if s.values == nil {
		s.values = map[string]string{}
	}
	s.values[key] = value
	return nil
}
func (s *identityStore) Delete(ctx context.Context, key string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	delete(s.values, key)
	return nil
}

func TestBoundCalendarConstructorRequiresVerifiedProviderIdentity(t *testing.T) {
	for _, profile := range []struct {
		name, scope string
		identity    bool
	}{{calendarCredentialName, calendarReadScope, true}, {credentialName, mailReadScope, false}, {teamsCredentialName, teamsReadScope, false}} {
		t.Run(profile.name, func(t *testing.T) {
			slot, err := integrations.CredentialSlot(profile.name, "synthetic-connection", trust.NewID())
			if err != nil {
				t.Fatal(err)
			}
			config := Config{ClientID: "synthetic-client", CredentialName: profile.name, Scope: profile.scope}
			runtime, err := New(context.Background(), &identityStore{}, config, slot)
			if err != nil {
				t.Fatal(err)
			}
			if profile.identity {
				runtime, err = NewCalendar(context.Background(), &identityStore{}, Config{ClientID: config.ClientID}, slot)
				if err != nil {
					t.Fatal(err)
				}
			}
			if runtime.requiresProviderIdentity() != profile.identity {
				t.Fatal("connection-bound slot changed profile identity requirements")
			}
			if hasScope(runtime.requestedScope(), openidScope) != profile.identity || hasScope(runtime.requestedScope(), profileScope) != profile.identity {
				t.Fatal("wrong profile authorization scopes")
			}
			if runtime.credentialKey() != slot || runtime.credentialNamespace != profile.name {
				t.Fatal("connection-scoped credential custody changed")
			}
			if profile.identity {
				if _, err := runtime.ProviderIdentity(context.Background()); err == nil {
					t.Fatal("missing identity admitted")
				}
			}
		})
	}
}

func TestCalendarBoundSlotVerifiesSignedIdentityBeforeRelease(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	const tenant = "12345678-1234-4234-8234-123456789abc"
	const client = "synthetic-client"
	const subject = "synthetic-subject"
	const nonce = "synthetic-nonce"
	issuer := "https://login.microsoftonline.com/" + tenant + "/v2.0"
	for _, test := range []struct {
		name   string
		valid  bool
		mutate func(map[string]any)
	}{
		{"valid", true, nil},
		{"wrong_nonce", false, func(c map[string]any) { c["nonce"] = "different" }},
		{"wrong_audience", false, func(c map[string]any) { c["aud"] = "different" }},
		{"wrong_issuer", false, func(c map[string]any) { c["iss"] = "https://example.invalid/issuer" }},
		{"expired", false, func(c map[string]any) { c["exp"] = time.Now().Add(-time.Minute).Unix() }},
		{"bad_signature", false, nil},
		{"missing_id_token", false, nil},
	} {
		t.Run(test.name, func(t *testing.T) {
			claims := map[string]any{"iss": issuer, "aud": client, "tid": tenant, "sub": subject, "nonce": nonce, "iat": time.Now().Add(-time.Second).Unix(), "exp": time.Now().Add(time.Hour).Unix()}
			if test.mutate != nil {
				test.mutate(claims)
			}
			header, _ := json.Marshal(map[string]string{"alg": "RS256", "kid": "synthetic-key"})
			payload, _ := json.Marshal(claims)
			signed := base64.RawURLEncoding.EncodeToString(header) + "." + base64.RawURLEncoding.EncodeToString(payload)
			digest := sha256.Sum256([]byte(signed))
			signature, err := rsa.SignPKCS1v15(rand.Reader, key, crypto.SHA256, digest[:])
			if err != nil {
				t.Fatal(err)
			}
			if test.name == "bad_signature" {
				signature[0] ^= 1
			}
			idToken := signed + "." + base64.RawURLEncoding.EncodeToString(signature)
			if test.name == "missing_id_token" {
				idToken = ""
			}
			var endpoint string
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "application/json")
				switch r.URL.Path {
				case "/token":
					if err := r.ParseForm(); err != nil {
						t.Error(err)
					}
					if !hasScope(r.Form.Get("scope"), openidScope) || !hasScope(r.Form.Get("scope"), profileScope) {
						t.Error("Calendar authorization omitted identity scopes")
					}
					_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "synthetic-access", "refresh_token": "synthetic-refresh", "expires_in": 3600, "scope": "Calendars.Read openid profile", "token_type": "Bearer", "id_token": idToken})
				case "/metadata":
					_ = json.NewEncoder(w).Encode(map[string]string{"issuer": "https://login.microsoftonline.com/common/v2.0", "jwks_uri": endpoint + "/keys"})
				case "/keys":
					_ = json.NewEncoder(w).Encode(map[string]any{"keys": []any{map[string]string{"kty": "RSA", "use": "sig", "alg": "RS256", "kid": "synthetic-key", "issuer": issuer, "n": base64.RawURLEncoding.EncodeToString(key.N.Bytes()), "e": base64.RawURLEncoding.EncodeToString(big.NewInt(int64(key.E)).Bytes())}}})
				default:
					t.Error("unexpected identity endpoint")
					http.NotFound(w, r)
				}
			}))
			defer server.Close()
			endpoint = server.URL
			slot, err := integrations.CredentialSlot(calendarCredentialName, "synthetic-calendar", trust.NewID())
			if err != nil {
				t.Fatal(err)
			}
			store := &identityStore{}
			runtime, err := NewCalendar(context.Background(), store, Config{ClientID: client}, slot)
			if err != nil {
				t.Fatal(err)
			}
			runtime.allowTestEndpoints = true
			runtime.tokenURL = endpoint + "/token"
			runtime.metadataURL = endpoint + "/metadata"
			bundle, err := runtime.tokenRequest(context.Background(), url.Values{"scope": {runtime.requestedScope()}}, nil, nonce)
			if !test.valid {
				if err == nil && bundle != nil && bundle.IdentityVerified {
					t.Fatal("invalid signed identity was verified")
				}
				if bundle != nil {
					if err := runtime.save(context.Background(), bundle); err != nil {
						t.Fatal(err)
					}
				}
				if token, err := runtime.Token(context.Background()); err == nil || token != "" {
					t.Fatal("Calendar token released without verified identity")
				}
				return
			}
			if err != nil || bundle == nil || !bundle.IdentityVerified || bundle.ProviderIdentity != "microsoft:"+tenant+":"+subject {
				t.Fatalf("valid signed identity failed: %v", err)
			}
			if err := runtime.save(context.Background(), bundle); err != nil {
				t.Fatal(err)
			}
			identity, err := runtime.ProviderIdentity(context.Background())
			if err != nil || identity != bundle.ProviderIdentity {
				t.Fatal("verified identity unavailable")
			}
			called := false
			if err := runtime.WithVerifiedProviderIdentity(slot, identity, func() error { called = true; return nil }); err != nil || !called {
				t.Fatal("exact identity gate did not release")
			}
			if err := runtime.WithVerifiedProviderIdentity(slot+"changed", identity, func() error { t.Fatal("foreign slot released"); return nil }); err == nil {
				t.Fatal("foreign slot admitted")
			}
			if err := runtime.WithVerifiedProviderIdentity(slot, identity+"changed", func() error { t.Fatal("foreign identity released"); return nil }); err == nil {
				t.Fatal("foreign identity admitted")
			}
			if token, err := runtime.Token(context.Background()); err != nil || token != "synthetic-access" {
				t.Fatal("verified Calendar token unavailable")
			}
			reopened, err := NewCalendar(context.Background(), store, Config{ClientID: client}, slot)
			if err != nil {
				t.Fatal(err)
			}
			reopened.allowTestEndpoints = true
			reopened.tokenURL = endpoint + "/token"
			reopened.metadataURL = endpoint + "/metadata"
			if token, err := reopened.Token(context.Background()); err == nil || token != "" {
				t.Fatal("persisted identity bypassed fresh verification")
			}
			if got, err := reopened.ProviderIdentity(context.Background()); err != nil || got != identity {
				t.Fatal("identity could not be reverified after recreation")
			}
		})
	}
}
