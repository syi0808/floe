package codexauth

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"sync"
	"testing"
	"time"
)

type memoryStore struct {
	mu     sync.Mutex
	values map[string]string
}

func (store *memoryStore) Get(key string) (string, error) {
	store.mu.Lock()
	defer store.mu.Unlock()
	return store.values[key], nil
}

func (store *memoryStore) Put(key, value string) error {
	store.mu.Lock()
	defer store.mu.Unlock()
	store.values[key] = value
	return nil
}

func (store *memoryStore) Delete(key string) error {
	store.mu.Lock()
	defer store.mu.Unlock()
	delete(store.values, key)
	return nil
}

func fixtureIDToken(account string) string {
	payload, _ := json.Marshal(map[string]any{
		"email":                       "fixture@example.com",
		"https://api.openai.com/auth": map[string]string{"chatgpt_account_id": account},
	})
	return "header." + base64.RawURLEncoding.EncodeToString(payload) + ".signature"
}

func TestDirectOAuthLoginStoresTokensAndLogoutDeletesThem(test *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	tokenServer := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodPost || request.FormValue("grant_type") != "authorization_code" || request.FormValue("code_verifier") == "" {
			test.Error("invalid token exchange")
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{
			"access_token": "access", "refresh_token": "refresh", "id_token": fixtureIDToken("account"), "expires_in": 3600,
		})
	}))
	defer tokenServer.Close()
	runtime := New(store)
	runtime.tokenURL = tokenServer.URL
	runtime.callbackAddress = "127.0.0.1:0"
	defer runtime.Close()
	ctx := context.Background()
	result, err := runtime.Action(ctx, "login")
	if err != nil || result.(map[string]any)["status"] != "pending" {
		test.Fatalf("login did not start: %v", err)
	}
	authorization, _ := url.Parse(result.(map[string]any)["auth_url"].(string))
	if authorization.Scheme != "https" || authorization.Host != "auth.openai.com" || authorization.Query().Get("code_challenge_method") != "S256" {
		test.Fatal("unsafe OAuth authorization URL")
	}
	runtime.mu.RLock()
	callback := "http://" + runtime.flow.listener.Addr().String() + "/auth/callback?state=" + url.QueryEscape(runtime.flow.state) + "&code=fixture"
	runtime.mu.RUnlock()
	response, err := http.Get(callback)
	if err != nil {
		test.Fatal(err)
	}
	response.Body.Close()
	result, err = runtime.Action(ctx, "status")
	if err != nil || result.(map[string]any)["status"] != "connected" || result.(map[string]any)["inference_enabled"] != true {
		test.Fatalf("OAuth completion was not retained: %v", err)
	}
	if encoded := store.values[credentialName]; !strings.Contains(encoded, "refresh") || strings.Contains(encoded, "code_verifier") {
		test.Fatal("wrong credential material stored")
	}
	result, err = runtime.Action(ctx, "logout")
	if err != nil || result.(map[string]any)["status"] != "disconnected" || store.values[credentialName] != "" {
		test.Fatal("logout did not delete tokens")
	}
	if _, err = runtime.Action(ctx, "thread/start"); err == nil {
		test.Fatal("unsupported OAuth operation accepted")
	}
}

func TestRefreshAndCodexInferenceUseServerOwnedCredential(test *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	refreshes := 0
	tokenServer := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		refreshes++
		if request.FormValue("grant_type") != "refresh_token" || request.FormValue("refresh_token") != "old-refresh" {
			test.Error("wrong refresh request")
		}
		_ = json.NewEncoder(writer).Encode(map[string]any{
			"access_token": "new-access", "refresh_token": "new-refresh", "id_token": fixtureIDToken("account-2"), "expires_in": 3600,
		})
	}))
	defer tokenServer.Close()
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer new-access" || request.Header.Get("ChatGPT-Account-Id") != "account-2" {
			test.Error("server-owned OAuth credential missing")
		}
		var body map[string]any
		if json.NewDecoder(request.Body).Decode(&body) != nil || body["tool_choice"] != "none" || len(body["tools"].([]any)) != 0 || body["parallel_tool_calls"] != true {
			test.Error("Codex request did not disable tools")
		}
		writer.Header().Set("Content-Type", "text/event-stream")
		_, _ = writer.Write([]byte("data: {\"type\":\"response.output_text.delta\",\"delta\":\"{\\\"ok\\\":\"}\n\n"))
		_, _ = writer.Write([]byte("data: {\"type\":\"response.output_text.delta\",\"delta\":\"true}\"}\n\n"))
		_, _ = writer.Write([]byte("data: {\"type\":\"response.output_text.done\",\"text\":\"{\\\"ok\\\":true}\"}\n\n"))
		_, _ = writer.Write([]byte("data: [DONE]\n\n"))
	}))
	defer upstream.Close()
	runtime := New(store)
	runtime.tokenURL = tokenServer.URL
	runtime.endpoint = upstream.URL
	if err := runtime.save(&tokenBundle{AccessToken: "old-access", RefreshToken: "old-refresh", IDToken: fixtureIDToken("account-1"), AccountID: "account-1", ExpiresAt: time.Now().Add(-time.Minute)}); err != nil {
		test.Fatal(err)
	}
	output, err := runtime.Generate(context.Background(), "fixture-model", "Return JSON", json.RawMessage(`{"test":true}`), json.RawMessage(`{"type":"object"}`))
	if err != nil || output != `{"ok":true}` || refreshes != 1 {
		test.Fatalf("direct Codex inference failed: %q %v", output, err)
	}
	if strings.Contains(store.values[credentialName], "old-access") {
		test.Fatal("refreshed credential was not persisted")
	}
}

func TestCallbackRejectsWrongStateAndOutputBounds(test *testing.T) {
	store := &memoryStore{values: map[string]string{}}
	runtime := New(store)
	runtime.callbackAddress = "127.0.0.1:0"
	defer runtime.Close()
	if _, err := runtime.Action(context.Background(), "login"); err != nil {
		test.Fatal(err)
	}
	runtime.mu.RLock()
	callback := "http://" + runtime.flow.listener.Addr().String() + "/auth/callback?state=wrong&code=fixture"
	runtime.mu.RUnlock()
	response, err := http.Get(callback)
	if err != nil || response.StatusCode != http.StatusBadRequest {
		test.Fatal("wrong OAuth state accepted")
	}
	response.Body.Close()
	if _, err := readResponse(strings.NewReader("data: {not-json}\n\n")); err == nil {
		test.Fatal("malformed provider output accepted")
	}
}
