package console

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

	"floe/server/internal/authorization"
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
	principal          authorization.Principal
	connectionID       string
	connectionRevision uint64
	expires            time.Time
}

func (console *Console) serveCalendarAuthority(writer http.ResponseWriter, request *http.Request, scope clientScope, calendars map[string]CalendarRuntime, connectionRecords map[string]connectionRecord) {
	if len(calendars) == 0 {
		failure(writer, http.StatusNotFound, "calendar_connector_not_found")
		return
	}
	if request.Method != http.MethodPost {
		failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
		return
	}
	principal := authorization.Principal{ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Authenticated: true}
	switch request.URL.Path {
	case "/v1/views/calendar.timeline":
		failure(writer, http.StatusBadRequest, "admission_required")
	case "/v1/views/calendar.timeline/admit":
		console.serveCalendarAdmission(writer, request, principal, calendars, connectionRecords)
	case "/v1/views/calendar.timeline/read":
		console.serveCalendarRead(writer, request, principal, calendars, connectionRecords)
	case "/v1/views/calendar.timeline/release":
		console.serveCalendarRelease(writer, request, principal)
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}

func (console *Console) serveCalendarAdmission(writer http.ResponseWriter, request *http.Request, principal authorization.Principal, calendars map[string]CalendarRuntime, connectionRecords map[string]connectionRecord) {
	var input calendarAdmissionWire
	if !decodeCalendarEnvelope(writer, request, map[string]struct{}{
		"schema_version": {}, "connector_id": {}, "connection_id": {}, "connection_revision": {},
		"resources": {}, "policy": {}, "grant": {}, "purpose": {}, "consumer": {},
		"max_items": {}, "max_bytes": {}, "query": {},
	}, &input) {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if input.SchemaVersion != authorization.SchemaVersion || input.ConnectorID != "calendar.google" && input.ConnectorID != "calendar.microsoft" || !validConnectionID(input.ConnectionID) || input.ConnectionRevision == 0 || len(input.Resources) != 1 || input.Resources[0] == "" || len(input.Resources[0]) > authorization.MaxResourceBytes || input.MaxItems == 0 || input.MaxItems > 128 || input.MaxBytes == 0 || input.MaxBytes > authorization.MaxStageBytesPerResult || input.Purpose == "" || input.Consumer == "" {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if err := validateCalendarObject(input.Query, map[string]struct{}{"range_start_unix_ms": {}, "range_end_unix_ms": {}, "cursor": {}, "limit": {}}); err != nil {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	var query calendarQueryWire
	if json.Unmarshal(input.Query, &query) != nil || query.RangeStartUnixMS < 0 || query.RangeEndUnixMS <= query.RangeStartUnixMS || query.RangeEndUnixMS-query.RangeStartUnixMS > int64(32*24*time.Hour/time.Millisecond) || len(query.Cursor) > 2048 || strings.ContainsAny(query.Cursor, "\r\n\x00") || query.Limit < 1 || query.Limit > 128 || uint32(query.Limit) > input.MaxItems {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	record, selected, ok := console.calendarRecord(principal, input, calendars, connectionRecords)
	if !ok || record.Scope == nil || record.Scope["calendar_id"] != input.Resources[0] {
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	if err := console.preflightCalendarIdentity(request.Context(), record); err != nil {
		failure(writer, http.StatusServiceUnavailable, "source_identity_unavailable")
		return
	}
	metadata, err := console.producerMetadata()
	if err != nil {
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	audience, ok := metadata["audience"].(string)
	if !ok || audience == "" {
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	queryDigest := sha256.Sum256(input.Query)
	authority := console.authorityEngine()
	if authority == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	challenge, err := authority.IssueAdmission(principal, authorization.Request{
		Audience: audience, Purpose: input.Purpose, Consumer: input.Consumer,
		Policy:    authorization.PolicyReference{Incarnation: input.Policy.Incarnation, Epoch: input.Policy.Epoch},
		Source:    authorization.SourceReference{ConnectorID: record.ConnectorID, ConnectionID: record.ConnectionID, ExecutionOwner: console.executionOwner(), Incarnation: record.Incarnation, Epoch: record.Epoch},
		Grant:     authorization.GrantReference{ID: input.Grant.ID, Incarnation: input.Grant.Incarnation, Epoch: input.Grant.Epoch},
		Resources: append([]string(nil), input.Resources...), QueryDigest: queryDigest, MaxItems: input.MaxItems, MaxBytes: input.MaxBytes,
	}, console)
	if err != nil {
		failure(writer, http.StatusForbidden, "admission_denied")
		return
	}
	console.mu.Lock()
	if console.calendarAdmissions == nil {
		console.calendarAdmissions = map[string]calendarAdmissionState{}
	}
	now := time.Now()
	clientAdmissions := 0
	for admissionID, admissionState := range console.calendarAdmissions {
		if !admissionState.expires.After(now) {
			delete(console.calendarAdmissions, admissionID)
			continue
		}
		if admissionState.principal.ClientID == principal.ClientID {
			clientAdmissions++
		}
	}
	if clientAdmissions >= authorization.MaxPendingPerClient {
		console.mu.Unlock()
		authority.CancelAdmission(challenge.ID)
		failure(writer, http.StatusConflict, "admission_capacity")
		return
	}
	console.calendarAdmissions[challenge.ID] = calendarAdmissionState{query: query, queryBytes: append([]byte(nil), input.Query...), principal: principal, connectionID: input.ConnectionID, connectionRevision: input.ConnectionRevision, expires: challenge.ExpiresAt}
	console.mu.Unlock()
	producerSignature := console.producer.signChallenge(challenge.Bytes)
	calendarChallengeReply(writer, http.StatusOK, "admission", challenge.ID, challenge.BytesB64, challenge.ExpiresAt, producerSignature, metadata)
	_ = selected
}

func (console *Console) serveCalendarRead(writer http.ResponseWriter, request *http.Request, principal authorization.Principal, calendars map[string]CalendarRuntime, connectionRecords map[string]connectionRecord) {
	proof, ok := decodeCalendarProof(writer, request)
	if !ok {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	authority := console.authorityEngine()
	if authority == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	admissionID, authorizedRequest, err := authority.ClaimAdmission(principal, proof, console)
	if err != nil {
		failure(writer, http.StatusForbidden, "admission_denied")
		return
	}
	console.mu.Lock()
	state, found := console.calendarAdmissions[admissionID]
	console.mu.Unlock()
	if !found || state.principal.ClientID != principal.ClientID || state.principal.PersonID != principal.PersonID || state.principal.DeviceID != principal.DeviceID || !state.expires.After(time.Now()) {
		authority.CancelAdmission(admissionID)
		failure(writer, http.StatusConflict, "admission_unavailable")
		return
	}
	if sha256.Sum256(state.queryBytes) != authorizedRequest.QueryDigest {
		authority.CancelAdmission(admissionID)
		console.deleteCalendarAdmission(admissionID)
		failure(writer, http.StatusForbidden, "release_denied")
		return
	}
	record, selected, ok := console.calendarRecordForRequest(principal, authorizedRequest, calendars, connectionRecords)
	if !ok || record.ConnectionID != state.connectionID || record.Revision != state.connectionRevision {
		authority.CancelAdmission(admissionID)
		console.deleteCalendarAdmission(admissionID)
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	if uint32(state.query.Limit) > authorizedRequest.MaxItems {
		authority.CancelAdmission(admissionID)
		console.deleteCalendarAdmission(admissionID)
		failure(writer, http.StatusForbidden, "release_denied")
		return
	}
	view, err := selected.ReadCalendarView(request.Context(), time.UnixMilli(state.query.RangeStartUnixMS), time.UnixMilli(state.query.RangeEndUnixMS), state.query.Cursor, state.query.Limit)
	if err != nil {
		authority.CancelAdmission(admissionID)
		console.deleteCalendarAdmission(admissionID)
		failure(writer, http.StatusServiceUnavailable, "view_unavailable")
		return
	}
	result, itemCount, err := boundedCalendarView(view)
	if err != nil {
		authority.CancelAdmission(admissionID)
		console.deleteCalendarAdmission(admissionID)
		failure(writer, http.StatusServiceUnavailable, "view_unavailable")
		return
	}
	release, err := authority.StageResult(admissionID, principal, authorizedRequest, result, itemCount)
	console.deleteCalendarAdmission(admissionID)
	if err != nil {
		failure(writer, http.StatusForbidden, "release_denied")
		return
	}
	metadata, err := console.producerMetadata()
	if err != nil {
		authority.CancelRelease(release.ID)
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	calendarChallengeReply(writer, http.StatusOK, "release", release.ID, release.BytesB64, release.ExpiresAt, console.producer.signChallenge(release.Bytes), metadata)
	_ = record
}

func (console *Console) serveCalendarRelease(writer http.ResponseWriter, request *http.Request, principal authorization.Principal) {
	proof, ok := decodeCalendarProof(writer, request)
	if !ok {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	authority := console.authorityEngine()
	if authority == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	result, err := authority.ClaimRelease(principal, proof, console)
	if err != nil {
		failure(writer, http.StatusForbidden, "release_denied")
		return
	}
	if len(result) == 0 || !json.Valid(result) {
		failure(writer, http.StatusServiceUnavailable, "view_unavailable")
		return
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": authorization.SchemaVersion, "view": json.RawMessage(result)})
}

func (console *Console) calendarRecord(principal authorization.Principal, input calendarAdmissionWire, calendars map[string]CalendarRuntime, connectionRecords map[string]connectionRecord) (connectionRecord, CalendarRuntime, bool) {
	record, exists := connectionRecords[input.ConnectionID]
	selected := calendars[input.ConnectionID]
	if !exists || selected == nil || record.ConnectorID != input.ConnectorID || record.PersonID != principal.PersonID || record.Device != nil && record.Device.DeviceID != principal.DeviceID {
		return connectionRecord{}, nil, false
	}
	return record, selected, true
}

func (console *Console) calendarRecordForRequest(principal authorization.Principal, request authorization.Request, calendars map[string]CalendarRuntime, connectionRecords map[string]connectionRecord) (connectionRecord, CalendarRuntime, bool) {
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
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, authorization.MaxChallengeBytes))
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

func decodeCalendarProof(writer http.ResponseWriter, request *http.Request) (authorization.Proof, bool) {
	var envelope calendarProofWire
	if !decodeCalendarEnvelope(writer, request, map[string]struct{}{"schema_version": {}, "proof": {}}, &envelope) || envelope.SchemaVersion != authorization.SchemaVersion || len(envelope.Proof) == 0 {
		return authorization.Proof{}, false
	}
	proof, err := authorization.ParseProofJSON(envelope.Proof)
	return proof, err == nil
}

func boundedCalendarView(view any) ([]byte, uint32, error) {
	result, err := json.Marshal(view)
	if err != nil || len(result) == 0 || len(result) > authorization.MaxStageBytesPerResult || !json.Valid(result) {
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
		"schema_version":     authorization.SchemaVersion,
		"operation":          operation,
		"challenge_id":       id,
		"challenge_b64url":   bytesB64,
		"producer_signature": base64.RawURLEncoding.EncodeToString(signature),
		"producer":           producer,
		"expires":            expires,
	})
}
