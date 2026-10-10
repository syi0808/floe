package gmail

import (
	"encoding/json"
	"errors"
	"floe/server/internal/adapters/storage/privatefiles"
	"floe/server/internal/integrations"
	"floe/server/internal/views"
	"os"
	"sort"
	"strings"
	"sync"
	"time"
)

const (
	indexVersion      = 1
	indexFileName     = "index.json"
	maxIndexItems     = 10_000
	maxIndexBytes     = 8 * 1024 * 1024
	maxLogisticsItems = 48
)

type Index struct {
	mu           sync.Mutex
	files        *storage.Files
	unavailable  bool
	connectionID string
	state        indexState
}

type indexState struct {
	SchemaVersion       int                   `json:"schema_version"`
	ConnectionID        string                `json:"connection_id"`
	HistoryID           string                `json:"history_id,omitempty"`
	Messages            map[string]Metadata   `json:"messages"`
	LastSuccessAtUnixMS *int64                `json:"last_success_at_unix_ms,omitempty"`
	LastFailure         *integrations.Failure `json:"last_failure,omitempty"`
}

func OpenIndex(files *storage.Files, connectionID string) (*Index, error) {
	if files == nil || !validID(connectionID) {
		return nil, ErrInvalidInput
	}
	index := &Index{
		files:        files,
		connectionID: connectionID,
		state:        indexState{SchemaVersion: indexVersion, ConnectionID: connectionID, Messages: map[string]Metadata{}},
	}
	data, err := files.Read(indexFileName, maxIndexBytes)
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
	next := indexState{SchemaVersion: indexVersion, ConnectionID: index.connectionID, HistoryID: historyID, Messages: map[string]Metadata{}, LastSuccessAtUnixMS: index.state.LastSuccessAtUnixMS, LastFailure: cloneFailure(index.state.LastFailure)}
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

func (index *Index) HistoryID() (string, error) {
	index.mu.Lock()
	defer index.mu.Unlock()
	if index.unavailable {
		return "", storage.ErrUnavailable
	}
	return index.state.HistoryID, nil
}

func (index *Index) RecordSync(now time.Time, failureKind string) error {
	index.mu.Lock()
	defer index.mu.Unlock()
	next := cloneIndexState(index.state)
	observed := now.UnixMilli()
	if failureKind == "" {
		next.LastSuccessAtUnixMS = &observed
		next.LastFailure = nil
	} else {
		if !validFailure(failureKind) {
			return ErrInvalidInput
		}
		next.LastFailure = &integrations.Failure{Kind: failureKind, ObservedAtUnixMS: observed}
	}
	return index.commit(next)
}

func (index *Index) Reset() error {
	index.mu.Lock()
	defer index.mu.Unlock()
	if index.unavailable {
		return storage.ErrUnavailable
	}
	return index.commit(indexState{SchemaVersion: indexVersion, ConnectionID: index.connectionID, Messages: map[string]Metadata{}})
}

func (index *Index) Communication(query string, cursor, limit int, now time.Time) (views.CommunicationView, error) {
	index.mu.Lock()
	defer index.mu.Unlock()
	if index.unavailable {
		return views.CommunicationView{}, storage.ErrUnavailable
	}
	if len(query) > 512 || cursor < 0 || limit < 1 || limit > MaxPageItems {
		return views.CommunicationView{}, ErrInvalidInput
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
		return views.CommunicationView{}, ErrInvalidInput
	}
	end := min(cursor+limit, len(matches))
	view := views.CommunicationView{SchemaVersion: 1, ViewID: "mail.communication", ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(5 * time.Minute).UnixMilli(), CoverageComplete: end == len(matches), Items: []views.CommunicationItem{}}
	view.SourceHandle, _ = SourceHandle(index.connectionID, "communication:"+index.state.HistoryID)
	if !view.CoverageComplete {
		next := end
		view.NextCursor = &next
	}
	for _, message := range matches[cursor:end] {
		evidence, _ := SourceHandle(index.connectionID, "message:"+message.ID)
		thread, _ := SourceHandle(index.connectionID, "thread:"+message.ThreadID)
		view.Items = append(view.Items, views.CommunicationItem{EvidenceHandle: evidence, ThreadHandle: thread, ReceivedUnixMS: message.ReceivedMS, From: message.From, To: message.To, Subject: message.Subject, Snippet: message.Snippet, Labels: append([]string(nil), message.Labels...)})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return views.CommunicationView{}, ErrInvalidResponse
	}
	return view, nil
}

func (index *Index) Logistics(now time.Time) (views.LogisticsView, error) {
	index.mu.Lock()
	defer index.mu.Unlock()
	if index.unavailable {
		return views.LogisticsView{}, storage.ErrUnavailable
	}
	matches := make([]Metadata, 0, maxLogisticsItems)
	for _, message := range index.state.Messages {
		if logisticsKind(message.Subject+"\n"+message.Snippet) != "" {
			matches = append(matches, message)
		}
	}
	sort.Slice(matches, func(left, right int) bool {
		if matches[left].ReceivedMS == matches[right].ReceivedMS {
			return matches[left].ID < matches[right].ID
		}
		return matches[left].ReceivedMS > matches[right].ReceivedMS
	})
	view := views.LogisticsView{
		SchemaVersion: 1, ViewID: "life.logistics", ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(5 * time.Minute).UnixMilli(), CoverageComplete: len(matches) <= maxLogisticsItems, Items: []views.LogisticsItem{},
	}
	view.SourceHandle, _ = SourceHandle(index.connectionID, "logistics:"+index.state.HistoryID)
	for _, message := range matches[:min(len(matches), maxLogisticsItems)] {
		summary := strings.TrimSpace(message.Subject)
		if summary == "" {
			summary = strings.TrimSpace(message.Snippet)
		}
		if len(summary) > 512 {
			summary = boundedText(summary, 512)
		}
		evidence, _ := SourceHandle(index.connectionID, "message:"+message.ID)
		view.Items = append(view.Items, views.LogisticsItem{EvidenceHandle: evidence, Kind: logisticsKind(message.Subject + "\n" + message.Snippet), Summary: summary, Status: "mail_candidate", NeedsAttention: false})
	}
	encoded, err := json.Marshal(view)
	if err != nil || len(encoded) > 65_536 {
		return views.LogisticsView{}, ErrInvalidResponse
	}
	return view, nil
}

func logisticsKind(value string) string {
	value = strings.ToLower(value)
	for _, candidate := range []struct {
		kind  string
		terms []string
	}{
		{"delivery", []string{"out for delivery", "has been delivered", "shipment", "tracking number", "your package", "your parcel", "배송", "택배"}},
		{"travel", []string{"flight confirmation", "boarding pass", "flight itinerary", "train ticket", "hotel confirmation", "항공", "탑승"}},
		{"reservation", []string{"reservation confirmed", "booking confirmation", "your reservation", "예약"}},
		{"errand", []string{"ready for pickup", "appointment reminder", "수령", "방문 예약"}},
	} {
		for _, term := range candidate.terms {
			if strings.Contains(value, term) {
				return candidate.kind
			}
		}
	}
	return ""
}

func boundedText(value string, maximum int) string {
	if len(value) <= maximum {
		return value
	}
	for maximum > 0 && maximum < len(value) && value[maximum]&0xc0 == 0x80 {
		maximum--
	}
	return strings.TrimSpace(value[:maximum])
}

func (index *Index) commit(next indexState) error {
	if index.unavailable {
		return storage.ErrUnavailable
	}
	data, err := json.Marshal(next)
	if err != nil || len(data) > maxIndexBytes {
		return ErrInvalidInput
	}
	if err := index.files.Write(indexFileName, data); err != nil {
		if storage.IsIndeterminate(err) || errors.Is(err, storage.ErrIntegrity) {
			index.unavailable = true
		}
		return err
	}
	index.state = next
	return nil
}

func (index *Index) validateState() error {
	if index.state.SchemaVersion != indexVersion || index.state.ConnectionID != index.connectionID || index.state.Messages == nil || len(index.state.Messages) > maxIndexItems || (index.state.HistoryID != "" && !validID(index.state.HistoryID)) || (index.state.LastSuccessAtUnixMS != nil && *index.state.LastSuccessAtUnixMS < 0) || (index.state.LastFailure != nil && (!validFailure(index.state.LastFailure.Kind) || index.state.LastFailure.ObservedAtUnixMS < 0)) {
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
	next := indexState{SchemaVersion: state.SchemaVersion, ConnectionID: state.ConnectionID, HistoryID: state.HistoryID, Messages: map[string]Metadata{}, LastSuccessAtUnixMS: state.LastSuccessAtUnixMS, LastFailure: cloneFailure(state.LastFailure)}
	for id, message := range state.Messages {
		next.Messages[id] = cloneMetadata(message)
	}
	return next
}

func cloneFailure(failure *integrations.Failure) *integrations.Failure {
	if failure == nil {
		return nil
	}
	copy := *failure
	return &copy
}

func (index *Index) checkAvailable() error {
	index.mu.Lock()
	defer index.mu.Unlock()
	if index.unavailable {
		return storage.ErrUnavailable
	}
	return index.files.Available()
}
