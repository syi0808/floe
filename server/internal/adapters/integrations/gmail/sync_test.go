package gmail

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	storage "floe/server/internal/adapters/storage/privatefiles"
)

type syntheticToken struct{}

func (syntheticToken) Token(context.Context) (string, error) { return "synthetic-local-qa", nil }

func testIndex(t *testing.T) (*Index, *storage.Files) {
	t.Helper()
	root := t.TempDir()
	if err := os.Chmod(root, 0700); err != nil {
		t.Fatal(err)
	}
	files, err := storage.NewFiles(root, "gmail-query-fixture", make([]byte, 32), true)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(files.Close)
	index, err := OpenIndex(files, "synthetic-connection")
	if err != nil {
		t.Fatal(err)
	}
	return index, files
}

func writeSyntheticMetadata(w http.ResponseWriter, id string) {
	_ = json.NewEncoder(w).Encode(map[string]any{
		"id": id, "threadId": "thread-" + id, "historyId": "100", "internalDate": "1000",
		"labelIds": []string{"INBOX"}, "snippet": "synthetic fixture", "payload": map[string]any{"headers": []any{}},
	})
}

func TestRefreshNeverImportsOutsideQueryMailboxChanges(t *testing.T) {
	var outsideReads atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		switch r.URL.Path {
		case "/users/me/profile":
			_, _ = w.Write([]byte(`{"historyId":"100"}`))
		case "/users/me/messages":
			if r.URL.Query().Get("q") != "label:chosen" {
				t.Errorf("wrong query: %s", r.URL.RawQuery)
			}
			_, _ = w.Write([]byte(`{"messages":[{"id":"inside","threadId":"thread-inside"}]}`))
		case "/users/me/history":
			_, _ = w.Write([]byte(`{"historyId":"100","history":[{"messagesAdded":[{"message":{"id":"outside","threadId":"thread-outside"}}]}]}`))
		default:
			id := strings.TrimPrefix(r.URL.Path, "/users/me/messages/")
			if id == "outside" {
				outsideReads.Add(1)
			}
			writeSyntheticMetadata(w, id)
		}
	}))
	defer server.Close()
	client, err := NewWithBaseURL(syntheticToken{}, server.URL)
	if err != nil {
		t.Fatal(err)
	}
	index, _ := testIndex(t)
	syncer, err := NewSyncer(client, index, "label:chosen")
	if err != nil {
		t.Fatal(err)
	}
	if err := syncer.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	view, err := index.Communication("", 0, 100, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if len(view.Items) != 1 || outsideReads.Load() != 0 {
		t.Fatalf("query-selected index imported unrelated history: items=%d outside metadata reads=%d", len(view.Items), outsideReads.Load())
	}
}

func newSyntheticSyncer(t *testing.T, index *Index, query string, handler http.HandlerFunc) *Syncer {
	t.Helper()
	server := httptest.NewServer(handler)
	t.Cleanup(server.Close)
	client, err := NewWithBaseURL(syntheticToken{}, server.URL)
	if err != nil {
		t.Fatal(err)
	}
	syncer, err := NewSyncer(client, index, query)
	if err != nil {
		t.Fatal(err)
	}
	return syncer
}

func seedSnapshot(t *testing.T, index *Index) {
	t.Helper()
	err := index.ReplaceQuerySnapshot([]Metadata{{MessageRef: MessageRef{ID: "previous", ThreadID: "thread-previous"}, HistoryID: "100", ReceivedMS: 1000}}, "label:previous")
	if err != nil {
		t.Fatal(err)
	}
}

func TestRefreshRechecksRelativeTimeMembershipWithoutHistoryChanges(t *testing.T) {
	index, files := testIndex(t)
	var expired atomic.Bool
	var historyCalls atomic.Int32
	syncer := newSyntheticSyncer(t, index, "newer_than:30d", func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/users/me/messages":
			if expired.Load() {
				_, _ = w.Write([]byte(`{"messages":[]}`))
			} else {
				_, _ = w.Write([]byte(`{"messages":[{"id":"inside","threadId":"thread-inside"}]}`))
			}
		case "/users/me/history":
			historyCalls.Add(1)
			_, _ = w.Write([]byte(`{"historyId":"100","history":[]}`))
		default:
			writeSyntheticMetadata(w, "inside")
		}
	})
	if err := syncer.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	first, err := index.Communication("", 0, 100, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	expired.Store(true)
	if err := syncer.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	second, err := index.Communication("", 0, 100, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if len(first.Items) != 1 || len(second.Items) != 0 || first.SourceHandle == second.SourceHandle || historyCalls.Load() != 0 {
		t.Fatal("expired query membership or source identity was retained")
	}
	reopened, err := OpenIndex(files, "synthetic-connection")
	if err != nil {
		t.Fatal(err)
	}
	restored, err := reopened.Communication("", 0, 100, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if len(restored.Items) != 0 || restored.SourceHandle != second.SourceHandle {
		t.Fatal("reopen lost query snapshot proof")
	}
}

func TestQueryAndMetadataIdentityBothChangeSnapshotProvenance(t *testing.T) {
	index, files := testIndex(t)
	var phase atomic.Int32
	syncer := newSyntheticSyncer(t, index, "label:first", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/users/me/messages" {
			_, _ = w.Write([]byte(`{"messages":[{"id":"same","threadId":"thread-same"}]}`))
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"id": "same", "threadId": "thread-same", "historyId": "100", "internalDate": "1000", "labelIds": []string{fmt.Sprint(phase.Load())}, "payload": map[string]any{"headers": []any{}}})
	})
	if err := syncer.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	first, _ := index.Communication("", 0, 100, time.Now())
	changed, err := NewSyncer(syncer.client, index, "label:second")
	if err != nil {
		t.Fatal(err)
	}
	if err := changed.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	second, _ := index.Communication("", 0, 100, time.Now())
	phase.Store(1)
	if err := changed.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	third, _ := index.Communication("", 0, 100, time.Now())
	if first.SourceHandle == second.SourceHandle || second.SourceHandle == third.SourceHandle {
		t.Fatal("query/metadata changes aliased one source handle")
	}
	reopened, err := OpenIndex(files, "synthetic-connection")
	if err != nil {
		t.Fatal(err)
	}
	restored, _ := reopened.Communication("", 0, 100, time.Now())
	if restored.SourceHandle != third.SourceHandle {
		t.Fatal("query-specific proof changed on reopen")
	}
}

func TestRefreshFailureNeverPartiallyReplacesTheEncryptedSnapshot(t *testing.T) {
	for _, failure := range []string{"metadata_failure", "wrong_message", "wrong_thread", "duplicate", "empty_page", "cursor_cycle", "overflow", "cancelled"} {
		t.Run(failure, func(t *testing.T) {
			index, files := testIndex(t)
			seedSnapshot(t, index)
			before, err := files.Read(indexFileName, maxIndexBytes)
			if err != nil {
				t.Fatal(err)
			}
			var calls atomic.Int32
			syncer := newSyntheticSyncer(t, index, "label:new", func(w http.ResponseWriter, r *http.Request) {
				calls.Add(1)
				if r.URL.Path == "/users/me/messages" {
					switch failure {
					case "duplicate":
						_, _ = w.Write([]byte(`{"messages":[{"id":"one","threadId":"thread-one"},{"id":"one","threadId":"thread-one"}]}`))
					case "empty_page":
						_, _ = w.Write([]byte(`{"messages":[],"nextPageToken":"again"}`))
					case "cursor_cycle":
						id := "first"
						if r.URL.Query().Get("pageToken") != "" {
							id = "second"
						}
						_ = json.NewEncoder(w).Encode(map[string]any{"messages": []map[string]string{{"id": id, "threadId": "thread-" + id}}, "nextPageToken": "again"})
					case "overflow":
						page := r.URL.Query().Get("pageToken")
						refs := []map[string]string{}
						for i := 0; i < 100; i++ {
							id := fmt.Sprintf("m-%s-%d", page, i)
							refs = append(refs, map[string]string{"id": id, "threadId": "thread-" + id})
						}
						_ = json.NewEncoder(w).Encode(map[string]any{"messages": refs, "nextPageToken": page + "n"})
					default:
						_, _ = w.Write([]byte(`{"messages":[{"id":"one","threadId":"thread-one"},{"id":"two","threadId":"thread-two"}]}`))
					}
					return
				}
				id := strings.TrimPrefix(r.URL.Path, "/users/me/messages/")
				if id == "two" {
					switch failure {
					case "metadata_failure":
						w.WriteHeader(http.StatusServiceUnavailable)
						return
					case "wrong_message":
						writeSyntheticMetadata(w, "another")
						return
					case "wrong_thread":
						_, _ = w.Write([]byte(`{"id":"two","threadId":"foreign-thread","historyId":"100","internalDate":"1000"}`))
						return
					}
				}
				writeSyntheticMetadata(w, id)
			})
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if failure == "cancelled" {
				cancel()
			}
			if err := syncer.Refresh(ctx); err == nil {
				t.Fatal("invalid or cancelled snapshot accepted")
			}
			after, err := files.Read(indexFileName, maxIndexBytes)
			if err != nil {
				t.Fatal(err)
			}
			if string(before) != string(after) {
				t.Fatal("partial failure rewrote prior snapshot")
			}
			view, err := index.Communication("", 0, 100, time.Now())
			if err != nil || len(view.Items) != 1 {
				t.Fatal("prior complete snapshot changed")
			}
			if failure == "cancelled" && calls.Load() != 0 {
				t.Fatal("cancelled refresh dispatched HTTP")
			}
		})
	}
}

func TestQuerySnapshotReopenRejectsLegacyOrCorruptProofWithoutReset(t *testing.T) {
	for _, field := range []string{"schema_version", "query_digest", "snapshot_digest"} {
		t.Run(field, func(t *testing.T) {
			index, files := testIndex(t)
			seedSnapshot(t, index)
			data, err := files.Read(indexFileName, maxIndexBytes)
			if err != nil {
				t.Fatal(err)
			}
			var state map[string]any
			if err := json.Unmarshal(data, &state); err != nil {
				t.Fatal(err)
			}
			if field == "schema_version" {
				state[field] = 1
			} else {
				state[field] = strings.Repeat("a", 64)
			}
			changed, err := json.Marshal(state)
			if err != nil {
				t.Fatal(err)
			}
			if err := files.Write(indexFileName, changed); err != nil {
				t.Fatal(err)
			}
			if _, err := OpenIndex(files, "synthetic-connection"); err == nil {
				t.Fatal("unproven index accepted")
			}
			retained, err := files.Read(indexFileName, maxIndexBytes)
			if err != nil || string(retained) != string(changed) {
				t.Fatal("unsupported state was reset or repaired")
			}
		})
	}
}

func TestEmptyUnobservedIndexIsNotACompleteView(t *testing.T) {
	index, _ := testIndex(t)
	if _, err := index.Communication("", 0, 100, time.Now()); !errors.Is(err, ErrUnavailable) {
		t.Fatal("unobserved communication marked available")
	}
	if _, err := index.Logistics(time.Now()); !errors.Is(err, ErrUnavailable) {
		t.Fatal("unobserved logistics marked available")
	}
}

func TestExactlyFiveHundredSelectedItemsRemainBoundedAndOrderIndependent(t *testing.T) {
	index, _ := testIndex(t)
	syncer := newSyntheticSyncer(t, index, "label:chosen", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/users/me/messages" {
			writeSyntheticMetadata(w, strings.TrimPrefix(r.URL.Path, "/users/me/messages/"))
			return
		}
		page := len(r.URL.Query().Get("pageToken"))
		refs := []map[string]string{}
		for i := 0; i < 100; i++ {
			id := fmt.Sprintf("m-%d", page*100+i)
			refs = append(refs, map[string]string{"id": id, "threadId": "thread-" + id})
		}
		next := ""
		if page < 4 {
			next = strings.Repeat("n", page+1)
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"messages": refs, "nextPageToken": next})
	})
	if err := syncer.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	view, err := index.Communication("", 0, 100, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if len(index.state.Messages) != 500 || len(view.Items) != 100 || view.CoverageComplete || view.NextCursor == nil || *view.NextCursor != 100 {
		t.Fatal("bounded page over exact full selection is incorrect")
	}
	messages := make([]Metadata, 0, 500)
	for _, m := range index.state.Messages {
		messages = append(messages, m)
	}
	if err := index.ReplaceQuerySnapshot(messages, "label:chosen"); err != nil {
		t.Fatal(err)
	}
	same, err := index.Communication("", 0, 100, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if same.SourceHandle != view.SourceHandle {
		t.Fatal("snapshot identity depends on provider enumeration order")
	}
}
