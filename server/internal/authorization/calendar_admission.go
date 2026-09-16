package authorization

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"reflect"
	"strings"
	"time"
)

const maxCalendarQueryBytes = 8 << 10

type calendarPolicyWire struct {
	Incarnation string `json:"incarnation"`
	Epoch       uint64 `json:"epoch"`
}

type calendarGrantWire struct {
	ID          string `json:"id"`
	Incarnation string `json:"incarnation"`
	Epoch       uint64 `json:"epoch"`
}

type calendarQueryWire struct {
	RangeStartUnixMS int64  `json:"range_start_unix_ms"`
	RangeEndUnixMS   int64  `json:"range_end_unix_ms"`
	Cursor           string `json:"cursor"`
	Limit            int    `json:"limit"`
}

type calendarAdmissionWire struct {
	SchemaVersion      int                `json:"schema_version"`
	ConnectorID        string             `json:"connector_id"`
	ConnectionID       string             `json:"connection_id"`
	ConnectionRevision uint64             `json:"connection_revision"`
	Resources          []string           `json:"resources"`
	Policy             calendarPolicyWire `json:"policy"`
	Grant              calendarGrantWire  `json:"grant"`
	Purpose            string             `json:"purpose"`
	Consumer           string             `json:"consumer"`
	MaxItems           uint32             `json:"max_items"`
	MaxBytes           uint32             `json:"max_bytes"`
	Query              json.RawMessage    `json:"query"`
}

type calendarProofWire struct {
	SchemaVersion int             `json:"schema_version"`
	Proof         json.RawMessage `json:"proof"`
}

type calendarAdmissionState struct {
	query              calendarQueryWire
	queryBytes         []byte
	principal          Principal
	connectionID       string
	connectionRevision uint64
	expires            time.Time
}

func (console *Console) calendarRecord(principal Principal, input calendarAdmissionWire, calendars map[string]CalendarRuntime, connectionRecords map[string]connectionRecord) (connectionRecord, CalendarRuntime, bool) {
	record, exists := connectionRecords[input.ConnectionID]
	selected := calendars[input.ConnectionID]
	if !exists || selected == nil || record.ConnectorID != input.ConnectorID || record.PersonID != principal.PersonID || record.Device != nil && record.Device.DeviceID != principal.DeviceID {
		return connectionRecord{}, nil, false
	}
	return record, selected, true
}

func (console *Console) calendarRecordForRequest(principal Principal, request Request, calendars map[string]CalendarRuntime, connectionRecords map[string]connectionRecord) (connectionRecord, CalendarRuntime, bool) {
	record, exists := connectionRecords[request.Source.ConnectionID]
	selected := calendars[request.Source.ConnectionID]
	if !exists || selected == nil || record.ConnectorID != request.Source.ConnectorID || record.PersonID != principal.PersonID || record.Incarnation != request.Source.Incarnation || record.Epoch != request.Source.Epoch || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.Scope == nil || len(request.Resources) != 1 || record.Scope["calendar_id"] != request.Resources[0] {
		return connectionRecord{}, nil, false
	}
	return record, selected, true
}

func (console *Console) preflightCalendarIdentity(ctx context.Context, expected connectionRecord) error {
	console.mu.Lock()
	current, exists := console.state.Connections[expected.ConnectionID]
	identityRuntime := console.calendarIdentityRuntimeLocked(expected.ConnectorID)
	console.mu.Unlock()
	if !exists || !sameCalendarRecord(current, expected) || current.IdentityUnverified || current.ProviderIdentity == "" || identityRuntime == nil {
		return errors.New("source identity unavailable")
	}
	provider, ok := identityRuntime.(ProviderIdentityRuntime)
	if !ok {
		return errors.New("source identity unavailable")
	}
	identityContext, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	identity, err := provider.ProviderIdentity(identityContext)
	if err != nil || identity == "" {
		return errors.New("source identity unavailable")
	}
	console.mu.Lock()
	defer console.mu.Unlock()
	latest, exists := console.state.Connections[expected.ConnectionID]
	if !exists || !sameCalendarRecord(latest, expected) || latest.IdentityUnverified || latest.ProviderIdentity != identity {
		return errors.New("source identity changed")
	}
	return nil
}

func sameCalendarRecord(first, second connectionRecord) bool {
	return first.ConnectionID == second.ConnectionID && first.Revision == second.Revision && first.ConnectorID == second.ConnectorID && first.PersonID == second.PersonID && first.Credential == second.Credential && first.Incarnation == second.Incarnation && first.Epoch == second.Epoch && first.ProviderIdentity == second.ProviderIdentity && first.IdentityUnverified == second.IdentityUnverified && reflect.DeepEqual(first.Device, second.Device) && reflect.DeepEqual(first.Scope, second.Scope)
}

func (console *Console) executionOwner() string {
	console.mu.Lock()
	defer console.mu.Unlock()
	return console.state.ExecutionOwnerID
}

func (console *Console) deleteCalendarAdmission(id string) {
	console.mu.Lock()
	delete(console.calendarAdmissions, id)
	console.mu.Unlock()
}

func decodeCalendarEnvelope(writer http.ResponseWriter, request *http.Request, allowed map[string]struct{}, output any) bool {
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, MaxChallengeBytes))
	if err != nil || len(data) == 0 || !strictAuthorityJSON(data) || !validateCalendarCaseExact(data) || !validateCalendarObjectKeys(data, allowed) {
		return false
	}
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.DisallowUnknownFields()
	if decoder.Decode(output) != nil {
		return false
	}
	var extra any
	return decoder.Decode(&extra) == io.EOF
}

func validateCalendarObject(data []byte, allowed map[string]struct{}) error {
	if len(data) == 0 || len(data) > maxCalendarQueryBytes || !strictAuthorityJSON(data) || !validateCalendarCaseExact(data) || !validateCalendarObjectKeys(data, allowed) {
		return errors.New("invalid calendar object")
	}
	return nil
}

var calendarFieldNames = map[string]struct{}{
	"schema_version": {}, "connector_id": {}, "connection_id": {}, "connection_revision": {},
	"resources": {}, "policy": {}, "grant": {}, "purpose": {}, "consumer": {},
	"max_items": {}, "max_bytes": {}, "query": {}, "range_start_unix_ms": {},
	"range_end_unix_ms": {}, "cursor": {}, "limit": {}, "incarnation": {},
	"epoch": {}, "id": {}, "proof": {}, "challenge_id": {}, "key_id": {}, "signature": {},
}

func validateCalendarCaseExact(data []byte) bool {
	var value any
	if json.Unmarshal(data, &value) != nil {
		return false
	}
	var visit func(any) bool
	visit = func(current any) bool {
		switch object := current.(type) {
		case map[string]any:
			for key, child := range object {
				for canonical := range calendarFieldNames {
					if strings.EqualFold(key, canonical) && key != canonical {
						return false
					}
				}
				if !visit(child) {
					return false
				}
			}
		case []any:
			for _, child := range object {
				if !visit(child) {
					return false
				}
			}
		}
		return true
	}
	return visit(value)
}

func validateCalendarObjectKeys(data []byte, allowed map[string]struct{}) bool {
	var raw map[string]json.RawMessage
	if json.Unmarshal(data, &raw) != nil {
		return false
	}
	for key := range raw {
		if _, ok := allowed[key]; !ok {
			return false
		}
	}
	return true
}

func decodeCalendarProof(writer http.ResponseWriter, request *http.Request) (Proof, bool) {
	var envelope calendarProofWire
	if !decodeCalendarEnvelope(writer, request, map[string]struct{}{"schema_version": {}, "proof": {}}, &envelope) || envelope.SchemaVersion != SchemaVersion || len(envelope.Proof) == 0 {
		return Proof{}, false
	}
	proof, err := ParseProofJSON(envelope.Proof)
	return proof, err == nil
}

func boundedCalendarView(view any) ([]byte, uint32, error) {
	result, err := json.Marshal(view)
	if err != nil || len(result) == 0 || len(result) > MaxStageBytesPerResult || !json.Valid(result) {
		return nil, 0, errors.New("invalid calendar view")
	}
	var envelope map[string]json.RawMessage
	if json.Unmarshal(result, &envelope) != nil {
		return nil, 0, errors.New("invalid calendar view")
	}
	itemCount := uint32(0)
	if rawItems, ok := envelope["items"]; ok {
		var items []json.RawMessage
		if json.Unmarshal(rawItems, &items) != nil || len(items) > 128 {
			return nil, 0, errors.New("invalid calendar items")
		}
		itemCount = uint32(len(items))
	}
	return result, itemCount, nil
}

func calendarChallengeReply(writer http.ResponseWriter, status int, operation, id, bytesB64 string, expires time.Time, signature []byte, metadata map[string]any) {
	producer := map[string]any{}
	for key, value := range metadata {
		producer[key] = value
	}
	reply(writer, status, map[string]any{
		"schema_version":     SchemaVersion,
		"operation":          operation,
		"challenge_id":       id,
		"challenge_b64url":   bytesB64,
		"producer_signature": base64.RawURLEncoding.EncodeToString(signature),
		"producer":           producer,
		"expires":            expires,
	})
}
