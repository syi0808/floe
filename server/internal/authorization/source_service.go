package authorization

import (
	"context"
	"crypto/rand"
	cryptorand "crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"time"

	"floe/server/internal/connections"
	"floe/server/internal/operation"
)

type SourceService struct {
	Admissions                *Admissions
	Authority                 SourceAuthority
	Engine                    func() *Engine
	Metadata                  func() (map[string]any, error)
	ExecutionOwner            func() string
	PreflightCalendarIdentity func(context.Context, connections.Record) error
	Records                   map[string]connections.Record
	Calendars                 map[string]connections.CalendarRuntime
	Communication             []connections.CommunicationRuntime
	Work                      []connections.WorkContextRuntime
	Logistics                 []connections.LogisticsRuntime
}

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

type CalendarAdmission struct {
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

type remoteViewAdmissionState struct {
	path          string
	query         []byte
	principal     Principal
	connectorID   string
	connectionID  string
	connectionRev uint64
	expires       time.Time
}

type ViewAdmission struct {
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

type SourcePreview struct {
	ConnectorID  string `json:"connector_id"`
	ConnectionID string `json:"connection_id"`
	Resource     string `json:"resource"`
}

func (service *SourceService) calendarRecord(principal Principal, input CalendarAdmission, calendars map[string]connections.CalendarRuntime, connectionRecords map[string]connections.Record) (connections.Record, connections.CalendarRuntime, bool) {
	record, exists := connectionRecords[input.ConnectionID]
	selected := calendars[input.ConnectionID]
	if !exists || selected == nil || record.ConnectorID != input.ConnectorID || record.PersonID != principal.PersonID || record.Device != nil && record.Device.DeviceID != principal.DeviceID {
		return connections.Record{}, nil, false
	}
	return record, selected, true
}
func (service *SourceService) calendarRecordForRequest(principal Principal, request Request, calendars map[string]connections.CalendarRuntime, connectionRecords map[string]connections.Record) (connections.Record, connections.CalendarRuntime, bool) {
	record, exists := connectionRecords[request.Source.ConnectionID]
	selected := calendars[request.Source.ConnectionID]
	if !exists || selected == nil || record.ConnectorID != request.Source.ConnectorID || record.PersonID != principal.PersonID || record.Incarnation != request.Source.Incarnation || record.Epoch != request.Source.Epoch || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.Scope == nil || len(request.Resources) != 1 || record.Scope["calendar_id"] != request.Resources[0] {
		return connections.Record{}, nil, false
	}
	return record, selected, true
}
func (service *SourceService) deleteCalendarAdmission(id string) {
	service.Admissions.mu.Lock()
	delete(service.Admissions.calendar, id)
	service.Admissions.mu.Unlock()
}
func validateCalendarObject(data []byte, allowed map[string]struct{}) error {
	if len(data) == 0 || len(data) > maxCalendarQueryBytes || !StrictJSON(data) || !ValidateCalendarCaseExact(data) || !ValidateCalendarObjectKeys(data, allowed) {
		return errors.New("invalid calendar object")
	}
	return nil
}
func ValidateCalendarCaseExact(data []byte) bool {
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
func ValidateCalendarObjectKeys(data []byte, allowed map[string]struct{}) bool {
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

var calendarFieldNames = map[string]struct{}{
	"schema_version": {}, "connector_id": {}, "connection_id": {}, "connection_revision": {},
	"resources": {}, "policy": {}, "grant": {}, "purpose": {}, "consumer": {},
	"max_items": {}, "max_bytes": {}, "query": {}, "range_start_unix_ms": {},
	"range_end_unix_ms": {}, "cursor": {}, "limit": {}, "incarnation": {},
	"epoch": {}, "id": {}, "proof": {}, "challenge_id": {}, "key_id": {}, "signature": {},
}

func remoteViewResource(viewID, connectionID string) string {
	return viewID + ":" + connectionID
}
func snapshotConnectionID(snapshot any) string {
	value, _, _, ok := connections.SnapshotMetadata(snapshot)
	if !ok {
		return ""
	}
	connection, ok := value["connection"].(map[string]any)
	if !ok {
		return ""
	}
	identifier, _ := connection["connection_id"].(string)
	return identifier
}
func (service *SourceService) readRemoteView(ctx context.Context, viewID, connectionID, connectorID string, query []byte, communication []connections.CommunicationRuntime, work []connections.WorkContextRuntime, logistics []connections.LogisticsRuntime) ([]byte, uint32, error) {
	var payload struct {
		Query  string `json:"query"`
		Cursor int    `json:"cursor"`
		Limit  int    `json:"limit"`
	}
	if viewID == "mail.communication" {
		decoder := json.NewDecoder(strings.NewReader(string(query)))
		decoder.DisallowUnknownFields()
		if decoder.Decode(&payload) != nil || decoder.Decode(new(any)) != io.EOF || len(payload.Query) > 512 || payload.Cursor < 0 || payload.Limit < 1 || payload.Limit > 100 {
			return nil, 0, errors.New("invalid mail query")
		}
		for _, runtime := range communication {
			if runtime == nil {
				continue
			}
			snapshot, err := runtime.ConnectionSnapshot(ctx)
			if err != nil {
				continue
			}
			_, connector, _, ok := connections.SnapshotMetadata(snapshot)
			if !ok || connector != connectorID || snapshotConnectionID(snapshot) != connectionID {
				continue
			}
			view, err := runtime.ReadCommunicationView(ctx, payload.Query, payload.Cursor, payload.Limit)
			if err == nil {
				encoded, marshalErr := json.Marshal(view)
				return encoded, viewItemCount(encoded), marshalErr
			}
		}
		return nil, 0, errors.New("mail unavailable")
	}
	if len(query) != len(`{"schema_version":1}`) || string(query) != `{"schema_version":1}` {
		return nil, 0, errors.New("invalid view query")
	}
	if viewID == "work.context" {
		for _, runtime := range work {
			if runtime == nil {
				continue
			}
			snapshot, err := runtime.ConnectionSnapshot(ctx)
			if err != nil {
				continue
			}
			_, connector, _, ok := connections.SnapshotMetadata(snapshot)
			if !ok || connector != connectorID || snapshotConnectionID(snapshot) != connectionID {
				continue
			}
			view, err := runtime.ReadWorkContextView(ctx)
			if err == nil {
				encoded, marshalErr := json.Marshal(view)
				return encoded, viewItemCount(encoded), marshalErr
			}
		}
	} else {
		for _, runtime := range logistics {
			if runtime == nil {
				continue
			}
			snapshot, err := runtime.ConnectionSnapshot(ctx)
			if err != nil {
				continue
			}
			_, connector, _, ok := connections.SnapshotMetadata(snapshot)
			if !ok || connector != connectorID || snapshotConnectionID(snapshot) != connectionID {
				continue
			}
			view, err := runtime.ReadLogisticsView(ctx)
			if err == nil {
				encoded, marshalErr := json.Marshal(view)
				return encoded, viewItemCount(encoded), marshalErr
			}
		}
	}
	return nil, 0, errors.New("view unavailable")
}
func viewItemCount(encoded []byte) uint32 {
	var envelope struct {
		Items []json.RawMessage `json:"items"`
	}
	if json.Unmarshal(encoded, &envelope) != nil || len(envelope.Items) > 128 {
		return 129
	}
	return uint32(len(envelope.Items))
}
func (service *SourceService) deleteRemoteViewAdmission(id string) {
	service.Admissions.mu.Lock()
	delete(service.Admissions.remoteView, id)
	service.Admissions.mu.Unlock()
}
func (service *SourceService) AdmitCalendar(ctx context.Context, principal Principal, input CalendarAdmission) (outcome operation.Result) {

	if input.SchemaVersion != SchemaVersion || input.ConnectorID != "calendar.google" && input.ConnectorID != "calendar.microsoft" || !validConnectionID(input.ConnectionID) || input.ConnectionRevision == 0 || len(input.Resources) != 1 || input.Resources[0] == "" || len(input.Resources[0]) > MaxResourceBytes || input.MaxItems == 0 || input.MaxItems > 128 || input.MaxBytes == 0 || input.MaxBytes > MaxStageBytesPerResult || input.Purpose == "" || input.Consumer == "" {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	if err := validateCalendarObject(input.Query, map[string]struct{}{"range_start_unix_ms": {}, "range_end_unix_ms": {}, "cursor": {}, "limit": {}}); err != nil {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	var query calendarQueryWire
	if json.Unmarshal(input.Query, &query) != nil || query.RangeStartUnixMS < 0 || query.RangeEndUnixMS <= query.RangeStartUnixMS || query.RangeEndUnixMS-query.RangeStartUnixMS > int64(32*24*time.Hour/time.Millisecond) || len(query.Cursor) > 2048 || strings.ContainsAny(query.Cursor, "\r\n\x00") || query.Limit < 1 || query.Limit > 128 || uint32(query.Limit) > input.MaxItems {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	record, selected, ok := service.calendarRecord(principal, input, service.Calendars, service.Records)
	if !ok || record.Scope == nil || record.Scope["calendar_id"] != input.Resources[0] {
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	if err := service.PreflightCalendarIdentity(ctx, record); err != nil {
		outcome = operation.Reject(operation.Unavailable, "source_identity_unavailable")
		return
	}
	metadata, err := service.Metadata()
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	audience, ok := metadata["audience"].(string)
	if !ok || audience == "" {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	queryDigest := sha256.Sum256(input.Query)
	authority := service.Engine()
	if authority == nil {
		outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
		return
	}
	challenge, err := authority.IssueAdmission(principal, Request{
		Audience: audience, Purpose: input.Purpose, Consumer: input.Consumer,
		Policy:    PolicyReference{Incarnation: input.Policy.Incarnation, Epoch: input.Policy.Epoch},
		Source:    SourceReference{ConnectorID: record.ConnectorID, ConnectionID: record.ConnectionID, ExecutionOwner: service.ExecutionOwner(), Incarnation: record.Incarnation, Epoch: record.Epoch},
		Grant:     GrantReference{ID: input.Grant.ID, Incarnation: input.Grant.Incarnation, Epoch: input.Grant.Epoch},
		Resources: append([]string(nil), input.Resources...), QueryDigest: queryDigest, MaxItems: input.MaxItems, MaxBytes: input.MaxBytes,
	}, service.Authority)
	if err != nil {
		outcome = operation.Reject(operation.Denied, "admission_denied")
		return
	}
	service.Admissions.mu.Lock()
	if service.Admissions.calendar == nil {
		service.Admissions.calendar = map[string]calendarAdmissionState{}
	}
	now := time.Now()
	clientAdmissions := 0
	for admissionID, admissionState := range service.Admissions.calendar {
		if !admissionState.expires.After(now) {
			delete(service.Admissions.calendar, admissionID)
			continue
		}
		if admissionState.principal.ClientID == principal.ClientID {
			clientAdmissions++
		}
	}
	if clientAdmissions >= MaxPendingPerClient {
		service.Admissions.mu.Unlock()
		authority.CancelAdmission(challenge.ID)
		outcome = operation.Reject(operation.Conflict, "admission_capacity")
		return
	}
	service.Admissions.calendar[challenge.ID] = calendarAdmissionState{query: query, queryBytes: append([]byte(nil), input.Query...), principal: principal, connectionID: input.ConnectionID, connectionRevision: input.ConnectionRevision, expires: challenge.ExpiresAt}
	service.Admissions.mu.Unlock()
	producerSignature := service.Admissions.Producer().SignChallenge(challenge.Bytes)
	outcome = operation.Accept(challengeReply("admission", challenge.ID, challenge.BytesB64, challenge.ExpiresAt, producerSignature, metadata))
	_ = selected
	return
}
func (service *SourceService) ReadCalendar(ctx context.Context, principal Principal, proof Proof) (outcome operation.Result) {
	authority := service.Engine()
	if authority == nil {
		outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
		return
	}
	admissionID, authorizedRequest, err := authority.ClaimAdmission(principal, proof, service.Authority)
	if err != nil {
		outcome = operation.Reject(operation.Denied, "admission_denied")
		return
	}
	service.Admissions.mu.Lock()
	state, found := service.Admissions.calendar[admissionID]
	service.Admissions.mu.Unlock()
	if !found || state.principal.ClientID != principal.ClientID || state.principal.PersonID != principal.PersonID || state.principal.DeviceID != principal.DeviceID || !state.expires.After(time.Now()) {
		authority.CancelAdmission(admissionID)
		outcome = operation.Reject(operation.Conflict, "admission_unavailable")
		return
	}
	if sha256.Sum256(state.queryBytes) != authorizedRequest.QueryDigest {
		authority.CancelAdmission(admissionID)
		service.deleteCalendarAdmission(admissionID)
		outcome = operation.Reject(operation.Denied, "release_denied")
		return
	}
	record, selected, ok := service.calendarRecordForRequest(principal, authorizedRequest, service.Calendars, service.Records)
	if !ok || record.ConnectionID != state.connectionID || record.Revision != state.connectionRevision {
		authority.CancelAdmission(admissionID)
		service.deleteCalendarAdmission(admissionID)
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	if uint32(state.query.Limit) > authorizedRequest.MaxItems {
		authority.CancelAdmission(admissionID)
		service.deleteCalendarAdmission(admissionID)
		outcome = operation.Reject(operation.Denied, "release_denied")
		return
	}
	view, err := selected.ReadCalendarView(ctx, time.UnixMilli(state.query.RangeStartUnixMS), time.UnixMilli(state.query.RangeEndUnixMS), state.query.Cursor, state.query.Limit)
	if err != nil {
		authority.CancelAdmission(admissionID)
		service.deleteCalendarAdmission(admissionID)
		outcome = operation.Reject(operation.Unavailable, "view_unavailable")
		return
	}
	result, itemCount, err := boundedCalendarView(view)
	if err != nil {
		authority.CancelAdmission(admissionID)
		service.deleteCalendarAdmission(admissionID)
		outcome = operation.Reject(operation.Unavailable, "view_unavailable")
		return
	}
	release, err := authority.StageResult(admissionID, principal, authorizedRequest, result, itemCount)
	service.deleteCalendarAdmission(admissionID)
	if err != nil {
		outcome = operation.Reject(operation.Denied, "release_denied")
		return
	}
	metadata, err := service.Metadata()
	if err != nil {
		authority.CancelRelease(release.ID)
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	outcome = operation.Accept(challengeReply("release", release.ID, release.BytesB64, release.ExpiresAt, service.Admissions.Producer().SignChallenge(release.Bytes), metadata))
	_ = record
	return
}
func (service *SourceService) Release(principal Principal, proof Proof) (outcome operation.Result) {
	authority := service.Engine()
	if authority == nil {
		outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
		return
	}
	result, err := authority.ClaimRelease(principal, proof, service.Authority)
	if err != nil {
		outcome = operation.Reject(operation.Denied, "release_denied")
		return
	}
	if len(result) == 0 || !json.Valid(result) {
		outcome = operation.Reject(operation.Unavailable, "view_unavailable")
		return
	}
	outcome = operation.Result{Category: operation.Ready, Value: map[string]any{"schema_version": SchemaVersion, "view": json.RawMessage(result)}}
	return
}
func (service *SourceService) PreviewView(principal Principal, viewID string, input SourcePreview) (outcome operation.Result) {
	if input.ConnectorID == "" || !validConnectionID(input.ConnectionID) || viewID != "calendar.timeline" && input.Resource != remoteViewResource(viewID, input.ConnectionID) || len(input.Resource) > MaxResourceBytes {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	record, ok := service.Records[input.ConnectionID]
	if !ok || record.ConnectorID != input.ConnectorID || record.PersonID != principal.PersonID || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.Revision == 0 || record.Incarnation == "" || record.Epoch == 0 || record.ProviderIdentity == "" || record.IdentityUnverified {
		outcome = operation.Reject(operation.Conflict, "source_unavailable")
		return
	}
	reference := SourceReference{ConnectorID: record.ConnectorID, ConnectionID: record.ConnectionID, ExecutionOwner: service.ExecutionOwner(), Incarnation: record.Incarnation, Epoch: record.Epoch}
	if err := service.Authority.WithCurrentSource(principal, reference, func(SourceSnapshot) error { return nil }); err != nil {
		outcome = operation.Reject(operation.Conflict, "source_unavailable")
		return
	}
	metadata, err := service.Metadata()
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	audience, ok := metadata["audience"].(string)
	if !ok || audience == "" {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	challengeID, err := newConnectionID()
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	descriptor := map[string]any{
		"v": 1, "operation": "remote_view_source_preview", "challenge_id": challengeID,
		"nonce": randomToken(), "view_id": viewID, "person_id": principal.PersonID,
		"client_id": principal.ClientID, "device_id": principal.DeviceID, "audience": audience,
		"connector_id": record.ConnectorID, "connection_id": record.ConnectionID,
		"connection_revision": record.Revision, "execution_owner": service.ExecutionOwner(),
		"incarnation": record.Incarnation, "epoch": record.Epoch, "resource": input.Resource,
		"provider_identity": record.ProviderIdentity, "issued_at_unix_ms": time.Now().UnixMilli(),
	}
	descriptorBytes, err := json.Marshal(descriptor)
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	metadata["descriptor_b64url"] = base64.RawURLEncoding.EncodeToString(descriptorBytes)
	metadata["producer_signature"] = base64.RawURLEncoding.EncodeToString(service.Admissions.Producer().SignChallenge(descriptorBytes))
	metadata["expires_at_unix_ms"] = time.Now().Add(30 * time.Second).UnixMilli()
	metadata["connection_revision"] = record.Revision
	outcome = operation.Result{Category: operation.Ready, Value: metadata}
	return
}
func (service *SourceService) AdmitView(principal Principal, viewID string, input ViewAdmission) (outcome operation.Result) {

	if input.SchemaVersion != SchemaVersion || !validConnectionID(input.ConnectionID) || input.ConnectionRevision == 0 || len(input.Resources) != 1 || input.Resources[0] != remoteViewResource(viewID, input.ConnectionID) || strings.TrimSpace(input.Resources[0]) == "" || len(input.Resources[0]) > MaxResourceBytes || input.MaxItems == 0 || input.MaxItems > 128 || input.MaxBytes == 0 || input.MaxBytes > MaxStageBytesPerResult || input.Purpose == "" || input.Consumer == "" || len(input.Query) == 0 || len(input.Query) > maxCalendarQueryBytes {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	if viewID == "mail.communication" {
		if input.ConnectorID != "gmail" && input.ConnectorID != "microsoft.mail" {
			outcome = operation.Reject(operation.Invalid, "validation")
			return
		}
	} else if input.ConnectorID == "" {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	record, exists := service.Records[input.ConnectionID]
	if !exists || record.ConnectorID != input.ConnectorID || record.PersonID != principal.PersonID || record.Revision != input.ConnectionRevision || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.Incarnation == "" || record.Epoch == 0 || record.ProviderIdentity == "" || record.IdentityUnverified {
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	queryDigest := sha256.Sum256(input.Query)
	authority := service.Engine()
	if authority == nil {
		outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
		return
	}
	metadata, err := service.Metadata()
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	audience, ok := metadata["audience"].(string)
	if !ok || audience == "" {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	challenge, err := authority.IssueAdmission(principal, Request{
		Audience: audience, Purpose: input.Purpose, Consumer: input.Consumer,
		Policy:    PolicyReference{Incarnation: input.Policy.Incarnation, Epoch: input.Policy.Epoch},
		Source:    SourceReference{ConnectorID: record.ConnectorID, ConnectionID: record.ConnectionID, ExecutionOwner: service.ExecutionOwner(), Incarnation: record.Incarnation, Epoch: record.Epoch},
		Grant:     GrantReference{ID: input.Grant.ID, Incarnation: input.Grant.Incarnation, Epoch: input.Grant.Epoch},
		Resources: append([]string(nil), input.Resources...), QueryDigest: queryDigest, MaxItems: input.MaxItems, MaxBytes: input.MaxBytes,
	}, service.Authority)
	if err != nil {
		outcome = operation.Reject(operation.Denied, "admission_denied")
		return
	}
	service.Admissions.mu.Lock()
	if service.Admissions.remoteView == nil {
		service.Admissions.remoteView = map[string]remoteViewAdmissionState{}
	}
	service.Admissions.remoteView[challenge.ID] = remoteViewAdmissionState{path: viewID, query: append([]byte(nil), input.Query...), principal: principal, connectorID: record.ConnectorID, connectionID: record.ConnectionID, connectionRev: input.ConnectionRevision, expires: challenge.ExpiresAt}
	service.Admissions.mu.Unlock()
	outcome = operation.Accept(challengeReply("admission", challenge.ID, challenge.BytesB64, challenge.ExpiresAt, service.Admissions.Producer().SignChallenge(challenge.Bytes), metadata))
	return
}
func (service *SourceService) ReadView(ctx context.Context, principal Principal, viewID string, proof Proof) (outcome operation.Result) {
	authority := service.Engine()
	if authority == nil {
		outcome = operation.Reject(operation.Unavailable, "authority_unavailable")
		return
	}
	admissionID, authorized, err := authority.ClaimAdmission(principal, proof, service.Authority)
	if err != nil {
		outcome = operation.Reject(operation.Denied, "admission_denied")
		return
	}
	service.Admissions.mu.Lock()
	state, found := service.Admissions.remoteView[admissionID]
	service.Admissions.mu.Unlock()
	if !found || state.path != viewID || state.principal != principal || !state.expires.After(time.Now()) || sha256.Sum256(state.query) != authorized.QueryDigest {
		authority.CancelAdmission(admissionID)
		outcome = operation.Reject(operation.Conflict, "admission_unavailable")
		return
	}
	record, exists := service.Records[state.connectionID]
	if !exists || record.ConnectorID != state.connectorID || record.PersonID != principal.PersonID || record.Revision != state.connectionRev || record.Incarnation != authorized.Source.Incarnation || record.Epoch != authorized.Source.Epoch || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.IdentityUnverified {
		authority.CancelAdmission(admissionID)
		service.deleteRemoteViewAdmission(admissionID)
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	result, items, err := service.readRemoteView(ctx, viewID, state.connectionID, state.connectorID, state.query, service.Communication, service.Work, service.Logistics)
	if err != nil {
		authority.CancelAdmission(admissionID)
		service.deleteRemoteViewAdmission(admissionID)
		outcome = operation.Reject(operation.Unavailable, "view_unavailable")
		return
	}
	release, err := authority.StageResult(admissionID, principal, authorized, result, items)
	service.deleteRemoteViewAdmission(admissionID)
	if err != nil {
		outcome = operation.Reject(operation.Denied, "release_denied")
		return
	}
	metadata, err := service.Metadata()
	if err != nil {
		authority.CancelRelease(release.ID)
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	outcome = operation.Accept(challengeReply("release", release.ID, release.BytesB64, release.ExpiresAt, service.Admissions.Producer().SignChallenge(release.Bytes), metadata))
	return
}
func (service *SourceService) PreviewCalendar(principal Principal, input SourcePreview) (outcome operation.Result) {
	if input.ConnectorID == "" || input.ConnectionID == "" || input.Resource == "" || len(input.Resource) > MaxResourceBytes {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	record, ok := service.Records[input.ConnectionID]
	if !ok || record.PersonID != principal.PersonID || record.ConnectorID != input.ConnectorID || record.IdentityUnverified || record.ProviderIdentity == "" || record.Epoch == 0 || record.Incarnation == "" || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.Scope == nil || record.Scope["calendar_id"] != input.Resource {
		outcome = operation.Reject(operation.Conflict, "source_unavailable")
		return
	}
	metadata, err := service.Metadata()
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	nonce := make([]byte, 32)
	if _, err := cryptorand.Read(nonce); err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	challengeID, err := newConnectionID()
	if err != nil {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	audience, _ := metadata["audience"].(string)
	executionOwner := service.ExecutionOwner()
	reference := SourceReference{ConnectorID: record.ConnectorID, ConnectionID: record.ConnectionID, ExecutionOwner: executionOwner, Incarnation: record.Incarnation, Epoch: record.Epoch}
	var descriptorBytes []byte
	var producerSignature []byte
	err = service.Authority.WithCurrentSource(principal, reference, func(SourceSnapshot) error {
		descriptor := map[string]any{
			"v": 1, "operation": "calendar_source_preview", "challenge_id": challengeID,
			"nonce": base64.RawURLEncoding.EncodeToString(nonce), "person_id": principal.PersonID,
			"client_id": principal.ClientID, "device_id": principal.DeviceID, "audience": audience,
			"connector_id": record.ConnectorID, "connection_id": record.ConnectionID,
			"execution_owner": executionOwner, "incarnation": record.Incarnation,
			"epoch": record.Epoch, "resource": input.Resource, "provider_identity": record.ProviderIdentity,
			"issued_at_unix_ms": time.Now().UnixMilli(),
		}
		var marshalError error
		descriptorBytes, marshalError = json.Marshal(descriptor)
		if marshalError != nil {
			return marshalError
		}
		producerSignature = service.Admissions.Producer().SignChallenge(descriptorBytes)
		return nil
	})
	if err != nil {
		outcome = operation.Reject(operation.Conflict, "source_unavailable")
		return
	}
	metadata["descriptor_b64url"] = base64.RawURLEncoding.EncodeToString(descriptorBytes)
	metadata["producer_signature"] = base64.RawURLEncoding.EncodeToString(producerSignature)
	metadata["expires_at_unix_ms"] = time.Now().Add(30 * time.Second).UnixMilli()
	outcome = operation.Result{Category: operation.Ready, Value: metadata}
	return
}
func challengeReply(operation, id, bytesB64 string, expires time.Time, signature []byte, metadata map[string]any) map[string]any {
	producer := map[string]any{}
	for key, value := range metadata {
		producer[key] = value
	}
	return map[string]any{
		"schema_version":     SchemaVersion,
		"operation":          operation,
		"challenge_id":       id,
		"challenge_b64url":   bytesB64,
		"producer_signature": base64.RawURLEncoding.EncodeToString(signature),
		"producer":           producer,
		"expires":            expires,
	}
}
func randomToken() string {
	return rand.Text() + rand.Text()
}

func newConnectionID() (string, error) {
	bytes := make([]byte, 16)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	bytes[6] = bytes[6]&0x0f | 0x40
	bytes[8] = bytes[8]&0x3f | 0x80
	hexadecimal := hex.EncodeToString(bytes)
	return hexadecimal[:8] + "-" + hexadecimal[8:12] + "-" + hexadecimal[12:16] + "-" + hexadecimal[16:20] + "-" + hexadecimal[20:], nil
}

func validConnectionID(value string) bool { return validateUUID(value) == nil && value[14] == '4' }
func StrictJSON(data []byte) bool         { return rejectDuplicateJSON(data) == nil }
