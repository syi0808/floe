package gmail

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"sync"
	"testing"
	"time"
)

func TestSyncerBootstrapsThenAppliesAddedChangedAndDeletedHistory(t *testing.T) {
	var mu sync.Mutex
	historyCalls := 0
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		mu.Lock()
		defer mu.Unlock()
		switch request.URL.Path {
		case "/users/me/profile":
			fmt.Fprint(writer, `{"historyId":"40"}`)
		case "/users/me/messages":
			fmt.Fprint(writer, `{"messages":[{"id":"m1","threadId":"t1"}]}`)
		case "/users/me/messages/m1":
			fmt.Fprint(writer, metadataJSON("m1", "t1", "Initial", "40"))
		case "/users/me/messages/m2":
			fmt.Fprint(writer, metadataJSON("m2", "t2", "Added", "41"))
		case "/users/me/history":
			historyCalls++
			if historyCalls == 1 {
				fmt.Fprint(writer, `{"history":[{"messagesAdded":[{"message":{"id":"m2","threadId":"t2"}}],"labelsAdded":[{"message":{"id":"m1","threadId":"t1"},"labelIds":["STARRED"]}]}],"historyId":"41"}`)
			} else {
				fmt.Fprint(writer, `{"history":[{"messagesDeleted":[{"message":{"id":"m1","threadId":"t1"}}]}],"historyId":"42"}`)
			}
		default:
			t.Fatalf("unexpected request: %s?%s", request.URL.Path, request.URL.RawQuery)
		}
	}))
	defer server.Close()
	directory := t.TempDir()
	os.Chmod(directory, 0700)
	index, _ := OpenIndex(directory, "account-1")
	client, _ := NewWithBaseURL(tokenSource("secret"), server.URL)
	syncer, err := NewSyncer(client, index, "newer_than:30d")
	if err != nil {
		t.Fatal(err)
	}
	if err := syncer.Bootstrap(context.Background()); err != nil {
		t.Fatal(err)
	}
	if index.HistoryID() != "41" {
		t.Fatalf("checkpoint: %s", index.HistoryID())
	}
	if err := syncer.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	view, err := index.Communication("", 0, 10, time.Unix(1_789_000_000, 0))
	if err != nil || len(view.Items) != 1 || view.Items[0].Subject != "Added" || index.HistoryID() != "42" {
		t.Fatalf("view: %#v %v", view, err)
	}
}

func TestRefreshRecoversExpiredCheckpointWithFullSync(t *testing.T) {
	var historyCalls int
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/users/me/history":
			historyCalls++
			if historyCalls == 1 {
				writer.WriteHeader(http.StatusNotFound)
			} else {
				fmt.Fprint(writer, `{"historyId":"51"}`)
			}
		case "/users/me/profile":
			fmt.Fprint(writer, `{"historyId":"50"}`)
		case "/users/me/messages":
			fmt.Fprint(writer, `{"messages":[]}`)
		default:
			t.Fatalf("unexpected request: %s", request.URL.Path)
		}
	}))
	defer server.Close()
	directory := t.TempDir()
	os.Chmod(directory, 0700)
	index, _ := OpenIndex(directory, "account-1")
	index.ApplyFull(nil, "40")
	client, _ := NewWithBaseURL(tokenSource("secret"), server.URL)
	syncer, _ := NewSyncer(client, index, "newer_than:30d")
	if err := syncer.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if index.HistoryID() != "51" || historyCalls != 2 {
		t.Fatalf("checkpoint=%s calls=%d", index.HistoryID(), historyCalls)
	}
}

func metadataJSON(id, thread, subject, history string) string {
	return fmt.Sprintf(`{"id":%q,"threadId":%q,"historyId":%q,"internalDate":"1789000000000","labelIds":["INBOX"],"snippet":"reply","payload":{"headers":[{"name":"Subject","value":%q}]}}`, id, thread, history, subject)
}
