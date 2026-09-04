package codexauth

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestAuthLifecycleUsingProtocolFixture(test *testing.T) {
	directory := test.TempDir()
	launcher := filepath.Join(directory, "codex-fixture")
	binary, _ := os.Executable()
	script := fmt.Sprintf("#!/bin/sh\nexec '%s' -test.run=TestCodexSubprocess -- floe-helper\n", strings.ReplaceAll(binary, "'", "'\\''"))
	if os.WriteFile(launcher, []byte(script), 0700) != nil {
		test.Fatal("cannot create fixture")
	}
	runtime := New(directory)
	runtime.executable = launcher
	defer runtime.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	state, err := runtime.Action(ctx, "status")
	if err != nil || state.(map[string]any)["status"] != "disconnected" {
		test.Fatalf("initialization: %v", err)
	}
	if _, err := runtime.Action(ctx, "thread/start"); err == nil {
		test.Fatal("arbitrary RPC allowed")
	}
	state, err = runtime.Action(ctx, "login")
	if err != nil || state.(map[string]any)["status"] != "connected" || state.(map[string]any)["auth_url"] != "" {
		test.Fatalf("completion notification lost: %v %v", state, err)
	}
	if state.(map[string]any)["inference_enabled"] != false {
		test.Fatal("inference enabled without isolation verification")
	}
	state, err = runtime.Action(ctx, "logout")
	if err != nil || state.(map[string]any)["status"] != "disconnected" {
		test.Fatalf("logout failed: %v", err)
	}
}

func TestCodexSubprocess(test *testing.T) {
	if os.Args[len(os.Args)-1] != "floe-helper" {
		return
	}
	if !strings.HasSuffix(os.Getenv("CODEX_HOME"), "codex-home") || os.Getenv("OPENAI_API_KEY") != "" || os.Getenv("FLOE_INFERENCE_TOKEN") != "" {
		os.Exit(3)
	}
	scanner := bufio.NewScanner(os.Stdin)
	encoder := json.NewEncoder(os.Stdout)
	connected := false
	for scanner.Scan() {
		var input message
		_ = json.Unmarshal(scanner.Bytes(), &input)
		var result any = map[string]any{}
		switch input.Method {
		case "initialized":
			continue
		case "initialize":
			result = map[string]string{"userAgent": "fixture"}
		case "account/read":
			var account any
			if connected {
				account = map[string]string{"type": "chatgpt"}
			}
			result = map[string]any{"account": account, "requiresOpenaiAuth": true}
		case "account/login/start":
			connected = true
			_ = encoder.Encode(map[string]any{"method": "account/login/completed", "params": map[string]any{"loginId": "fixture-login", "success": true}})
			result = map[string]string{"loginId": "fixture-login", "authUrl": "https://auth.openai.com/oauth/authorize?state=fixture"}
		case "account/logout":
			connected = false
		default:
			os.Exit(4)
		}
		_ = encoder.Encode(map[string]any{"id": input.ID, "result": result})
	}
	os.Exit(0)
}

func TestAuthURLAllowlist(test *testing.T) {
	for _, address := range []string{"https://evil.example", "http://auth.openai.com", "https://auth.openai.com@evil.example", "https://auth.openai.com:444/oauth", "javascript:alert(1)"} {
		if validAuthURL(address) {
			test.Fatal("unsafe authorization URL")
		}
	}
}

func TestInstalledCodexHandshake(test *testing.T) {
	if os.Getenv("FLOE_TEST_INSTALLED_CODEX") != "1" {
		test.Skip("explicit installed-runtime smoke test")
	}
	runtime := New(test.TempDir())
	defer runtime.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	state, err := runtime.Action(ctx, "status")
	if err != nil {
		test.Fatal(err)
	}
	if state.(map[string]any)["status"] != "disconnected" {
		test.Fatal("isolated runtime inherited credentials")
	}
}
