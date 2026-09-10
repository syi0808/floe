package gmail

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
)

const (
	indexVersion  = 1
	maxIndexItems = 10_000
	maxIndexBytes = 8 * 1024 * 1024
)

type Index struct {
	mu           sync.Mutex
	path         string
	connectionID string
	state        indexState
}

type indexState struct {
	SchemaVersion int                 `json:"schema_version"`
	ConnectionID  string              `json:"connection_id"`
	HistoryID     string              `json:"history_id,omitempty"`
	Messages      map[string]Metadata `json:"messages"`
}

type CommunicationItem struct {
	EvidenceHandle string   `json:"evidence_handle"`
	ThreadHandle   string   `json:"thread_handle"`
	ReceivedUnixMS int64    `json:"received_unix_ms"`
	From           string   `json:"from,omitempty"`
	To             string   `json:"to,omitempty"`
	Subject        string   `json:"subject,omitempty"`
	Snippet        string   `json:"snippet,omitempty"`
	Labels         []string `json:"labels"`
}

type CommunicationView struct {
	SchemaVersion    int                 `json:"schema_version"`
	ViewID           string              `json:"view_id"`
	SourceHandle     string              `json:"source_handle"`
	ObservedAtUnixMS int64               `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS  int64               `json:"expires_at_unix_ms"`
	CoverageComplete bool                `json:"coverage_complete"`
	NextCursor       *int                `json:"next_cursor,omitempty"`
	Items            []CommunicationItem `json:"items"`
}

func OpenIndex(directory, connectionID string) (*Index, error) {
	if !validID(connectionID) {
		return nil, ErrInvalidInput
	}
	if err := ensurePrivateDirectory(directory); err != nil {
		return nil, err
	}
	name, _ := SourceHandle(connectionID, "index")
	index := &Index{
		path:         filepath.Join(directory, name+".json"),
		connectionID: connectionID,
		state:        indexState{SchemaVersion: indexVersion, ConnectionID: connectionID, Messages: map[string]Metadata{}},
	}
	data, err := readPrivateFile(index.path)
	if os.IsNotExist(err) {
		return index, nil
	}
	if err != nil || len(data) > maxIndexBytes || json.Unmarshal(data, &index.state) != nil || index.validateState() != nil {
		return nil, errors.New("invalid gmail index")
	}
	return index, nil
}

func (index *Index) ApplyFull(messages []Metadata, historyID string) error {
	index.mu.Lock()
	defer index.mu.Unlock()
	if !validID(historyID) || len(messages) > maxIndexItems {
		return ErrInvalidInput
	}
	next := indexState{SchemaVersion: indexVersion, ConnectionID: index.connectionID, HistoryID: historyID, Messages: map[string]Metadata{}}
	for _, message := range messages {
		if validateMetadata(message) != nil || next.Messages[message.ID].ID != "" {
			return ErrInvalidInput
		}
		next.Messages[message.ID] = cloneMetadata(message)
	}
	return index.commit(next)
}

func (index *Index) ApplyDelta(upserts []Metadata, deletedIDs []string, previousHistoryID, historyID string) error {
	index.mu.Lock()
	defer index.mu.Unlock()
	if !validID(previousHistoryID) || !validID(historyID) || index.state.HistoryID != previousHistoryID {
		return ErrInvalidInput
	}
	next := cloneIndexState(index.state)
	for _, message := range upserts {
		if validateMetadata(message) != nil {
			return ErrInvalidInput
		}
		next.Messages[message.ID] = cloneMetadata(message)
	}
	seen := map[string]bool{}
	for _, id := range deletedIDs {
		if !validID(id) || seen[id] {
			return ErrInvalidInput
		}
		seen[id] = true
		delete(next.Messages, id)
	}
	if len(next.Messages) > maxIndexItems {
		return ErrInvalidInput
	}
	next.HistoryID = historyID
	return index.commit(next)
}

func (index *Index) HistoryID() string {
	index.mu.Lock()
	defer index.mu.Unlock()
	return index.state.HistoryID
}

func (index *Index) Communication(query string, cursor, limit int, now time.Time) (CommunicationView, error) {
	index.mu.Lock()
	defer index.mu.Unlock()
	if len(query) > 512 || cursor < 0 || limit < 1 || limit > MaxPageItems {
		return CommunicationView{}, ErrInvalidInput
	}
	needle := strings.ToLower(strings.TrimSpace(query))
	matches := make([]Metadata, 0, len(index.state.Messages))
	for _, message := range index.state.Messages {
		haystack := strings.ToLower(message.From + "\x00" + message.To + "\x00" + message.Subject + "\x00" + message.Snippet)
		if needle == "" || strings.Contains(haystack, needle) {
			matches = append(matches, message)
		}
	}
	sort.Slice(matches, func(left, right int) bool {
		if matches[left].ReceivedMS == matches[right].ReceivedMS {
			return matches[left].ID < matches[right].ID
		}
		return matches[left].ReceivedMS > matches[right].ReceivedMS
	})
	if cursor > len(matches) {
		return CommunicationView{}, ErrInvalidInput
	}
	end := min(cursor+limit, len(matches))
	view := CommunicationView{SchemaVersion: 1, ViewID: "mail.communication", ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(5 * time.Minute).UnixMilli(), CoverageComplete: end == len(matches), Items: []CommunicationItem{}}
	view.SourceHandle, _ = SourceHandle(index.connectionID, "communication:"+index.state.HistoryID)
	if !view.CoverageComplete {
		next := end
		view.NextCursor = &next
	}
	for _, message := range matches[cursor:end] {
		evidence, _ := SourceHandle(index.connectionID, "message:"+message.ID)
		thread, _ := SourceHandle(index.connectionID, "thread:"+message.ThreadID)
		view.Items = append(view.Items, CommunicationItem{EvidenceHandle: evidence, ThreadHandle: thread, ReceivedUnixMS: message.ReceivedMS, From: message.From, To: message.To, Subject: message.Subject, Snippet: message.Snippet, Labels: append([]string(nil), message.Labels...)})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return CommunicationView{}, ErrInvalidResponse
	}
	return view, nil
}

func (index *Index) commit(next indexState) error {
	data, err := json.Marshal(next)
	if err != nil || len(data) > maxIndexBytes {
		return ErrInvalidInput
	}
	if err := writePrivateFile(index.path, data); err != nil {
		return err
	}
	index.state = next
	return nil
}

func (index *Index) validateState() error {
	if index.state.SchemaVersion != indexVersion || index.state.ConnectionID != index.connectionID || index.state.Messages == nil || len(index.state.Messages) > maxIndexItems || (index.state.HistoryID != "" && !validID(index.state.HistoryID)) {
		return ErrInvalidInput
	}
	for id, message := range index.state.Messages {
		if id != message.ID || validateMetadata(message) != nil {
			return ErrInvalidInput
		}
	}
	return nil
}

func validateMetadata(message Metadata) error {
	if !validID(message.ID) || !validID(message.ThreadID) || !validID(message.HistoryID) || message.ReceivedMS < 0 || len(message.Labels) > 128 || len(message.From) > 4096 || len(message.To) > 4096 || len(message.Subject) > 4096 || len(message.Snippet) > 1024 {
		return ErrInvalidInput
	}
	return nil
}

func cloneMetadata(message Metadata) Metadata {
	message.Labels = append([]string(nil), message.Labels...)
	return message
}
func cloneIndexState(state indexState) indexState {
	next := indexState{SchemaVersion: state.SchemaVersion, ConnectionID: state.ConnectionID, HistoryID: state.HistoryID, Messages: map[string]Metadata{}}
	for id, message := range state.Messages {
		next.Messages[id] = cloneMetadata(message)
	}
	return next
}

func ensurePrivateDirectory(directory string) error {
	if err := os.MkdirAll(directory, 0700); err != nil {
		return err
	}
	info, err := os.Lstat(directory)
	if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 || info.Mode().Perm()&0077 != 0 {
		return errors.New("gmail index directory must be private")
	}
	return nil
}

func readPrivateFile(path string) ([]byte, error) {
	info, err := os.Lstat(path)
	if err != nil {
		return nil, err
	}
	if !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 {
		return nil, errors.New("gmail index file must be private")
	}
	return os.ReadFile(path)
}

func writePrivateFile(path string, data []byte) error {
	file, err := os.CreateTemp(filepath.Dir(path), ".gmail-*")
	if err != nil {
		return err
	}
	temporary := file.Name()
	defer os.Remove(temporary)
	if _, err = file.Write(data); err != nil {
		file.Close()
		return err
	}
	if err = file.Sync(); err != nil {
		file.Close()
		return err
	}
	if err = file.Close(); err != nil {
		return err
	}
	return os.Rename(temporary, path)
}
