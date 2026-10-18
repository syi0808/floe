package httptransport

import (
	"context"
	"floe/server/internal/inference"
	"net"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

type liveRustAccount struct{ *liveHTTPAccount }

func (a liveRustAccount) LiveProfile() (inference.LiveProfile, bool) {
	p, _ := a.liveObservationAccount.LiveProfile()
	p.Limits.InitialContextBytes = 2 * 1024 * 1024
	p.ProtocolSource = "openai/codex@0df4aa67781ae45a3c97befa4db48b734afe46d9"
	return p, true
}
func TestRustLiveControlToGoSyntheticTransport(t *testing.T) {
	if os.Getenv("FLOE_LIVE_RUN_RUST_E2E") != "1" {
		t.Skip("explicit Rust/Go cross-language gate")
	}
	f := newConsoleFixture(t)
	cookie, csrf := f.login(t)
	bearer := completeSyntheticPairing(t, f, cookie, csrf)
	principal, err := f.trust.AuthenticateBearer(context.Background(), bearer)
	if err != nil {
		t.Fatal(err)
	}
	a := &liveHTTPAccount{call: &liveHTTPTransport{done: make(chan struct{})}}
	if err = f.handler.Inference.Service.Configure(inference.InferenceConfig{Routes: map[inference.Purpose]inference.PurposeRoute{inference.LivePresentation: {TargetID: "live", Enabled: true}}}, map[string]inference.ModelAccount{"live": liveRustAccount{a}}, noLiveGeneration{}); err != nil {
		t.Fatal(err)
	}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address := listener.Addr().String()
	f.address = address
	f.handler.Address = address
	f.handler.Inference.Address = address
	gateway := httptest.NewUnstartedServer(f.handler)
	gateway.Listener = listener
	gateway.Start()
	defer gateway.Close()
	_, source, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("test source unavailable")
	}
	root := filepath.Clean(filepath.Join(filepath.Dir(source), "../../../.."))
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()
	command := exec.CommandContext(ctx, "cargo", "test", "-p", "floe-provider-adapters", "--lib", "gateway::live::tests::rust_live_control_to_go_e2e", "--", "--exact", "--ignored", "--nocapture")
	command.Dir = root
	command.Env = []string{"PATH=" + os.Getenv("PATH"), "CARGO_HOME=" + os.Getenv("CARGO_HOME"), "RUSTUP_HOME=" + os.Getenv("RUSTUP_HOME"), "CARGO_BUILD_JOBS=2", "TMPDIR=" + os.TempDir(), "FLOE_LIVE_ENDPOINT=" + gateway.URL, "FLOE_LIVE_BEARER=" + bearer, "FLOE_LIVE_PERSON=" + principal.PersonID()}
	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("Rust live control gate failed: %v\n%s", err, output)
	}
	if a.starts.Load() != 1 || a.call.acks.Load() != 1 || a.call.writes.Load() != 1 || a.call.closes.Load() != 1 {
		t.Fatalf("unexpected call/ACK/append/close counts: %d/%d/%d/%d", a.starts.Load(), a.call.acks.Load(), a.call.writes.Load(), a.call.closes.Load())
	}
}
