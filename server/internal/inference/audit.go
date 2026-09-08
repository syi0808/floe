package inference

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"sync"
	"time"
)

type AuditRecord struct {
	TraceID          string    `json:"trace_id"`
	CreatedAt        time.Time `json:"created_at"`
	Purpose          string    `json:"purpose"`
	DataClasses      []string  `json:"data_classes"`
	Placement        string    `json:"placement"`
	ExternalTransfer bool      `json:"external_transfer"`
	RequestDigest    string    `json:"request_digest"`
	ResponseDigest   string    `json:"response_digest,omitempty"`
	Outcome          string    `json:"outcome"`
	ReplayOf         string    `json:"replay_of,omitempty"`
}

type auditLog struct {
	mu      sync.Mutex
	limit   int
	order   []string
	records map[string]AuditRecord
}

func newAuditLog(limit int) *auditLog {
	return &auditLog{limit: limit, records: make(map[string]AuditRecord)}
}

func (log *auditLog) add(record AuditRecord) {
	log.mu.Lock()
	defer log.mu.Unlock()
	if len(log.order) == log.limit {
		delete(log.records, log.order[0])
		log.order = log.order[1:]
	}
	log.order = append(log.order, record.TraceID)
	log.records[record.TraceID] = record
}

func (log *auditLog) get(identifier string) (AuditRecord, bool) {
	log.mu.Lock()
	defer log.mu.Unlock()
	record, exists := log.records[identifier]
	return record, exists
}

func (log *auditLog) list(limit int) []AuditRecord {
	log.mu.Lock()
	defer log.mu.Unlock()
	if limit > len(log.order) {
		limit = len(log.order)
	}
	records := make([]AuditRecord, 0, limit)
	for index := len(log.order) - 1; index >= len(log.order)-limit; index-- {
		records = append(records, log.records[log.order[index]])
	}
	return records
}

func requestDigest(request Request) string {
	request.ReplayOf = ""
	encoded, _ := json.Marshal(request)
	digest := sha256.Sum256(encoded)
	return hex.EncodeToString(digest[:])
}

func newAuditRecord(traceID string, request Request, placement, outcome, output string) AuditRecord {
	record := AuditRecord{
		TraceID:          traceID,
		CreatedAt:        time.Now().UTC(),
		Purpose:          request.Purpose,
		DataClasses:      append([]string(nil), request.DataClasses...),
		Placement:        placement,
		ExternalTransfer: placement == "remote",
		RequestDigest:    requestDigest(request),
		Outcome:          outcome,
		ReplayOf:         request.ReplayOf,
	}
	if output != "" {
		responseDigest := sha256.Sum256([]byte(output))
		record.ResponseDigest = hex.EncodeToString(responseDigest[:])
	}
	return record
}
