package inference

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"sync"
	"time"
)

type AuditRecord struct {
 TraceID string `json:"trace_id"`
 CreatedAt time.Time `json:"created_at"`
 Purpose string `json:"purpose"`
 DataClasses []string `json:"data_classes"`
 RequestDigest string `json:"request_digest"`
 ResponseDigest string `json:"response_digest,omitempty"`
 IdentityDigest string `json:"identity_digest"`
 Outcome string `json:"outcome"`
 Usage UsageObservation `json:"usage"`
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

func newAuditRecord(trace,purpose string,classes []string,request any,identity,outcome string,output any,usage UsageObservation,started time.Time)AuditRecord {
 raw,_:=json.Marshal(request);digest:=sha256.Sum256(raw);id:=sha256.Sum256([]byte(identity));record:=AuditRecord{TraceID:trace,CreatedAt:started.UTC(),Purpose:purpose,DataClasses:append([]string(nil),classes...),RequestDigest:hex.EncodeToString(digest[:]),IdentityDigest:hex.EncodeToString(id[:]),Outcome:outcome,Usage:usage};if output!=nil{raw,_=json.Marshal(output);digest=sha256.Sum256(raw);record.ResponseDigest=hex.EncodeToString(digest[:])};return record
}
