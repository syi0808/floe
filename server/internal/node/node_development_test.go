//go:build floe_dev

package node

import (
	"bytes"
	"context"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"testing"
	"time"
)

func configurationCleanupWorkers(t *testing.T) int {
	t.Helper()
	var stack bytes.Buffer
	if err := pprof.Lookup("goroutine").WriteTo(&stack, 2); err != nil {
		t.Fatal(err)
	}
	return bytes.Count(stack.Bytes(), []byte("floe/server/internal/inference.(*Configuration).cleanupLoop("))
}

func TestFailedStartupClosesConfigurationBeforeReleasingStorage(t *testing.T) {
	config := Config{Directory: filepath.Join(t.TempDir(), "profile"), Address: "127.0.0.1:18431"}
	before := configurationCleanupWorkers(t)
	n, err := New(config)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := n.Close(); err != nil {
			t.Error(err)
		}
	})
	files, err := n.storage.files.Scope("integrations")
	if err != nil {
		t.Fatal(err)
	}
	// An authenticated but invalid owner payload fails after Configuration opened.
	// Keep it in place: startup must neither reset it nor leave a live owner worker.
	if err := files.Write("integrations.json", []byte("not-json")); err != nil {
		t.Fatal(err)
	}
	n.Close()
	for i := 0; i < 3; i++ {
		failed, err := New(config)
		if failed != nil {
			failed.Close()
			t.Fatal("invalid integration state admitted")
		}
		if err == nil || err.Error() != "integration state unavailable" {
			t.Fatalf("unexpected startup result: %v", err)
		}
	}
	vault, err := profileStore(config.Directory, false)
	if err != nil {
		t.Fatal(err)
	}
	storage, err := openStorage(context.Background(), config.Directory, vault, false)
	if err != nil {
		t.Fatal(err)
	}
	defer storage.Close()
	preserved, err := storage.files.Scope("integrations")
	if err != nil {
		t.Fatal(err)
	}
	data, err := preserved.Read("integrations.json", 1024)
	if err != nil || string(data) != "not-json" {
		t.Fatalf("failed startup altered owner state: %q %v", data, err)
	}
	deadline := time.Now().Add(time.Second)
	for {
		runtime.Gosched()
		workers := configurationCleanupWorkers(t)
		if workers == before {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("failed startup leaked configuration workers: before=%d after=%d", before, workers)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestSuccessfulStartupRetainsConfigurationUntilClose(t *testing.T) {
	before := configurationCleanupWorkers(t)
	config := Config{Directory: filepath.Join(t.TempDir(), "profile"), Address: "127.0.0.1:18431"}
	n, err := New(config)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := n.Close(); err != nil {
			t.Error(err)
		}
	})
	runtime.Gosched()
	if got := configurationCleanupWorkers(t); got != before+1 {
		t.Fatalf("configuration lifetime: got %d want %d", got, before+1)
	}
	n.Close()
	n.Close()
	if got := configurationCleanupWorkers(t); got != before {
		t.Fatalf("configuration survived close: %d", got)
	}
}

func TestCloseBoundsAnAdmittedRequestThatIgnoresCancellation(t *testing.T) {
	n, err := New(Config{Directory: filepath.Join(t.TempDir(), "profile"), Address: "127.0.0.1:18431"})
	if err != nil {
		t.Fatal(err)
	}
	files, err := n.storage.files.Scope("integrations")
	if err != nil {
		t.Fatal(err)
	}
	if err = files.Write("shutdown-sentinel.json", []byte("synthetic retained IO")); err != nil {
		t.Fatal(err)
	}
	// Simulate an admitted handler stuck in an external boundary that ignores
	// request cancellation. New admissions are fenced, but this task still owns IO.
	n.active.Add(1)
	released := false
	release := func() {
		if !released {
			released = true
			n.active.Done()
		}
	}
	defer release()
	done := make(chan error, 1)
	go func() { done <- n.Close() }()
	select {
	case err := <-done:
		if err == nil {
			t.Fatal("undrained request reported successful shutdown")
		}
	case <-time.After(6 * time.Second):
		release()
		<-done
		t.Fatal("Close exceeded its shutdown bound while waiting for an admitted request")
	}
	if data, err := files.Read("shutdown-sentinel.json", 1024); err != nil || string(data) != "synthetic retained IO" {
		t.Fatal("storage closed underneath an admitted request", err)
	}
	response := httptest.NewRecorder()
	n.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "http://127.0.0.1:18431/manage/", nil))
	if response.Code != http.StatusServiceUnavailable {
		t.Fatal("shutdown admitted a new request", response.Code)
	}
	release()
	select {
	case <-n.closeDone:
	case <-time.After(2 * time.Second):
		t.Fatal("late request drain did not release resources")
	}
	if _, err := files.Read("shutdown-sentinel.json", 1024); err == nil {
		t.Fatal("drained node retained live storage")
	}
	if err := n.Close(); err == nil {
		t.Fatal("retry erased the original incomplete-shutdown evidence")
	}
}
