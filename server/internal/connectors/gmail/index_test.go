package gmail

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func indexed(id, thread, subject string, received int64) Metadata {
	return Metadata{MessageRef: MessageRef{ID: id, ThreadID: thread}, HistoryID: "40", ReceivedMS: received, Labels: []string{"INBOX"}, From: "sender@example.com", To: "me@example.com", Subject: subject, Snippet: "Please reply by Friday"}
}

func TestIndexPersistsMetadataAppliesCheckpointedDeltaAndProjectsBoundedView(t *testing.T) {
	directory := t.TempDir()
	if err := os.Chmod(directory, 0700); err != nil {
		t.Fatal(err)
	}
	index, err := OpenIndex(directory, "account-1")
	if err != nil {
		t.Fatal(err)
	}
	first := indexed("m1", "t1", "Launch", 20)
	second := indexed("m2", "t2", "Budget", 10)
	if err := index.ApplyFull([]Metadata{first, second}, "40"); err != nil {
		t.Fatal(err)
	}
	if err := index.ApplyDelta([]Metadata{indexed("m3", "t3", "Launch follow-up", 30)}, []string{"m2"}, "40", "41"); err != nil {
		t.Fatal(err)
	}
	if err := index.ApplyDelta(nil, nil, "40", "42"); !errors.Is(err, ErrInvalidInput) {
		t.Fatalf("stale checkpoint: %v", err)
	}

	reopened, err := OpenIndex(directory, "account-1")
	if err != nil || reopened.HistoryID() != "41" {
		t.Fatalf("reopen: %v %q", err, reopened.HistoryID())
	}
	now := time.Unix(1_789_000_000, 0)
	view, err := reopened.Communication("launch", 0, 1, now)
	if err != nil || view.CoverageComplete || view.NextCursor == nil || len(view.Items) != 1 {
		t.Fatalf("first view: %#v %v", view, err)
	}
	if strings.Contains(view.Items[0].EvidenceHandle, "m3") || strings.Contains(view.SourceHandle, "account-1") {
		t.Fatalf("native identity leaked: %#v", view)
	}
	next, err := reopened.Communication("launch", *view.NextCursor, 1, now)
	if err != nil || !next.CoverageComplete || len(next.Items) != 1 {
		t.Fatalf("next view: %#v %v", next, err)
	}
}

func TestIndexNeverPersistsBodiesAndRejectsUnsafeStorage(t *testing.T) {
	directory := t.TempDir()
	if err := os.Chmod(directory, 0700); err != nil {
		t.Fatal(err)
	}
	index, err := OpenIndex(directory, "account-1")
	if err != nil {
		t.Fatal(err)
	}
	message := indexed("m1", "t1", "Secret", 20)
	if err := index.ApplyFull([]Metadata{message}, "40"); err != nil {
		t.Fatal(err)
	}
	files, _ := filepath.Glob(filepath.Join(directory, "*.json"))
	if len(files) != 1 {
		t.Fatalf("files: %v", files)
	}
	data, err := os.ReadFile(files[0])
	if err != nil || !json.Valid(data) || strings.Contains(string(data), "body.data") {
		t.Fatalf("unsafe index: %s %v", data, err)
	}
	if err := os.Chmod(files[0], 0644); err != nil {
		t.Fatal(err)
	}
	if _, err := OpenIndex(directory, "account-1"); err == nil {
		t.Fatal("accepted public index file")
	}

	publicDirectory := filepath.Join(t.TempDir(), "public")
	if err := os.Mkdir(publicDirectory, 0755); err != nil {
		t.Fatal(err)
	}
	if _, err := OpenIndex(publicDirectory, "account-1"); err == nil {
		t.Fatal("accepted public directory")
	}
}

func TestIndexRejectsCorruptAndCrossConnectionState(t *testing.T) {
	directory := t.TempDir()
	if err := os.Chmod(directory, 0700); err != nil {
		t.Fatal(err)
	}
	index, _ := OpenIndex(directory, "account-1")
	if err := index.ApplyFull([]Metadata{indexed("m1", "t1", "Launch", 20)}, "40"); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(index.path)
	var state map[string]any
	if json.Unmarshal(data, &state) != nil {
		t.Fatal("decode")
	}
	state["connection_id"] = "account-2"
	data, _ = json.Marshal(state)
	if err := os.WriteFile(index.path, data, 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := OpenIndex(directory, "account-1"); err == nil {
		t.Fatal("accepted cross-connection state")
	}
}

func TestIndexProjectsOnlyExplicitLogisticsMailCandidates(t *testing.T) {
	directory := t.TempDir()
	os.Chmod(directory, 0700)
	index, _ := OpenIndex(directory, "account-1")
	delivery := indexed("m1", "t1", "Your package is out for delivery", 30)
	travel := indexed("m2", "t2", "Flight confirmation ICN to SFO", 20)
	ordinary := indexed("m3", "t3", "Software package review", 10)
	if err := index.ApplyFull([]Metadata{delivery, travel, ordinary}, "40"); err != nil {
		t.Fatal(err)
	}
	view, err := index.Logistics(time.Unix(1_789_128_000, 0))
	if err != nil || len(view.Items) != 2 || view.Items[0].Kind != "delivery" || view.Items[1].Kind != "travel" {
		t.Fatalf("view: %#v %v", view, err)
	}
	encoded := fmt.Sprintf("%#v", view)
	for _, private := range []string{"account-1", "m1", "t1"} {
		if strings.Contains(encoded, private) {
			t.Fatalf("provider identity leaked: %s", private)
		}
	}
}
