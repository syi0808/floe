package workoauth

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

type memoryStore map[string]string

func (store memoryStore) Get(key string) (string, error) { return store[key], nil }
func (store memoryStore) Put(key, value string) error    { store[key] = value; return nil }
func (store memoryStore) Delete(key string) error        { delete(store, key); return nil }

func TestGitHubAppDeviceFlowPersistsBoundTokenWithoutSecret(t *testing.T) {
	store := memoryStore{}
	var deviceForm, tokenForm url.Values
	tokenRequests := 0
	provider := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodPost || request.Header.Get("Accept") != "application/json" {
			t.Fatalf("provider request: %s %#v", request.Method, request.Header)
		}
		_ = request.ParseForm()
		writer.Header().Set("Content-Type", "application/json")
		if request.URL.Path == "/device" {
			deviceForm = request.Form
			fmt.Fprint(writer, `{"device_code":"github-device-code","user_code":"ABCD-EFGH","verification_uri":"https://github.example.test/login/device","expires_in":900,"interval":5}`)
			return
		}
		tokenForm = request.Form
		tokenRequests++
		if tokenRequests == 1 {
			fmt.Fprint(writer, `{"error":"authorization_pending"}`)
			return
		}
		fmt.Fprint(writer, `{"access_token":"github-access-token","refresh_token":"github-refresh-token","expires_in":28800,"refresh_token_expires_in":15897600,"token_type":"bearer"}`)
	}))
	defer provider.Close()
	runtime, err := NewGitHub(store, Config{ClientID: "github-client", ClientSecret: "must-not-be-used"})
	if err != nil {
		t.Fatal(err)
	}
	defer runtime.Close()
	runtime.profile.authURL = provider.URL + "/device"
	runtime.profile.tokenURL = provider.URL + "/token"
	if err := runtime.BindCredential(GitHubCredential + ":" + strings.Repeat("a", 64)); err != nil {
		t.Fatal(err)
	}
	result, err := runtime.Action(context.Background(), "login")
	if err != nil {
		t.Fatal(err)
	}
	status := result.(map[string]any)
	if status["auth_url"] != "https://github.example.test/login/device" || status["user_code"] != "ABCD-EFGH" || deviceForm.Get("client_id") != "github-client" {
		t.Fatalf("device status=%#v form=%#v", status, deviceForm)
	}
	runtime.mu.Lock()
	runtime.flow.nextPoll = time.Now()
	runtime.mu.Unlock()
	result, err = runtime.Action(context.Background(), "status")
	if err != nil {
		t.Fatal(err)
	}
	if result.(map[string]any)["status"] != "pending" || tokenRequests != 1 {
		t.Fatalf("pending status=%#v requests=%d", result, tokenRequests)
	}
	result, err = runtime.Action(context.Background(), "status")
	if err != nil || tokenRequests != 1 {
		t.Fatalf("early poll status=%#v requests=%d error=%v", result, tokenRequests, err)
	}
	runtime.mu.Lock()
	runtime.flow.nextPoll = time.Now()
	runtime.mu.Unlock()
	result, err = runtime.Action(context.Background(), "status")
	if err != nil {
		t.Fatal(err)
	}
	if result.(map[string]any)["status"] != "connected" || tokenForm.Get("client_secret") != "" ||
		tokenForm.Get("device_code") != "github-device-code" ||
		tokenForm.Get("grant_type") != "urn:ietf:params:oauth:grant-type:device_code" {
		t.Fatalf("status=%#v token=%#v", result, tokenForm)
	}
	token, err := runtime.Token(context.Background())
	if err != nil || token != "github-access-token" {
		t.Fatalf("token=%q error=%v", token, err)
	}
}

func TestSlackDesktopPKCEUsesUserScopesWithoutSecret(t *testing.T) {
	store := memoryStore{}
	var tokenForm url.Values
	tokenServer := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		_ = request.ParseForm()
		tokenForm = request.Form
		writer.Header().Set("Content-Type", "application/json")
		if request.Form.Get("grant_type") == "refresh_token" {
			fmt.Fprint(writer, `{"ok":true,"access_token":"slack-rotated-token","refresh_token":"slack-rotated-refresh","scope":"channels:history,groups:history","token_type":"user","expires_in":43200}`)
			return
		}
		fmt.Fprint(writer, `{"ok":true,"authed_user":{"access_token":"slack-access-token","refresh_token":"slack-refresh-token","scope":"channels:history,groups:history","token_type":"user","expires_in":43200}}`)
	}))
	defer tokenServer.Close()
	runtime, err := NewSlack(store, Config{ClientID: "slack-client"})
	if err != nil {
		t.Fatal(err)
	}
	defer runtime.Close()
	runtime.profile.authURL = "https://slack.example.test/authorize"
	runtime.profile.tokenURL = tokenServer.URL
	runtime.callbackAddress = "127.0.0.1:0"
	result, err := runtime.Action(context.Background(), "login")
	if err != nil {
		t.Fatal(err)
	}
	authURL, _ := url.Parse(result.(map[string]any)["auth_url"].(string))
	if authURL.Query().Get("user_scope") != "channels:history,groups:history" || !strings.HasPrefix(authURL.Query().Get("redirect_uri"), "http://localhost:") {
		t.Fatalf("authorization query: %s", authURL.RawQuery)
	}
	callback := authURL.Query().Get("redirect_uri") + "?code=fixture-code&state=" + url.QueryEscape(authURL.Query().Get("state"))
	response, err := http.Get(callback)
	if err != nil {
		t.Fatal(err)
	}
	_ = response.Body.Close()
	if response.StatusCode != http.StatusOK || tokenForm.Get("client_secret") != "" || tokenForm.Get("code_verifier") == "" {
		t.Fatalf("callback=%d token=%#v", response.StatusCode, tokenForm)
	}
	token, err := runtime.Token(context.Background())
	if err != nil || token != "slack-access-token" {
		t.Fatalf("token=%q error=%v", token, err)
	}
	runtime.mu.Lock()
	runtime.tokens.ExpiresAt = time.Now()
	runtime.mu.Unlock()
	token, err = runtime.Token(context.Background())
	if err != nil || token != "slack-rotated-token" || tokenForm.Get("refresh_token") != "slack-refresh-token" {
		t.Fatalf("rotated token=%q form=%#v error=%v", token, tokenForm, err)
	}
}
