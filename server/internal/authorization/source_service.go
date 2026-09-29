package authorization

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"reflect"
	"regexp"
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
const maxViewSourcePreviewProofBytes = 16 << 10

func calendarScopeResources(record connections.Record) ([]string, bool) {
	definition, ok := connections.DefinitionFor(record.ConnectorID)
	if !ok || !connections.IsCalendarConnector(record.ConnectorID) {
		return nil, false
	}
	calendarIDs, ok := connections.ConnectorScopeStrings(record.Scope["calendar_ids"])
	if !ok {
		return nil, false
	}
	validated, err := connections.ValidatedConnectorScope(definition, record.Scope)
	if err != nil || !reflect.DeepEqual(calendarIDs, validated["calendar_ids"]) {
		return nil, false
	}
	return calendarIDs, true
}

type viewGrantWire struct {
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

func parseCalendarQuery(data json.RawMessage, maxItems uint32) (calendarQueryWire, error) {
	if err := validateCalendarObject(data, map[string]struct{}{"range_start_unix_ms": {}, "range_end_unix_ms": {}, "cursor": {}, "limit": {}}); err != nil {
		return calendarQueryWire{}, err
	}
	var query calendarQueryWire
	if json.Unmarshal(data, &query) != nil || query.RangeStartUnixMS < 0 || query.RangeEndUnixMS <= query.RangeStartUnixMS || query.RangeEndUnixMS-query.RangeStartUnixMS > int64(32*24*time.Hour/time.Millisecond) || len(query.Cursor) > 2048 || strings.ContainsAny(query.Cursor, "\r\n\x00") || query.Limit < 1 || query.Limit > 128 || uint32(query.Limit) > maxItems {
		return calendarQueryWire{}, errors.New("invalid calendar query")
	}
	return query, nil
}

type remoteViewAdmissionState struct {
	path             string
	query            []byte
	principal        Principal
	connectorID      string
	connectionID     string
	connectionRev    uint64
	sourceResources  []string
	providerIdentity string
	expires          time.Time
}

type ViewAdmission struct {
	SchemaVersion      int             `json:"schema_version"`
	ConnectorID        string          `json:"connector_id"`
	ConnectionID       string          `json:"connection_id"`
	ConnectionRevision uint64          `json:"connection_revision"`
	Resources          []string        `json:"resources"`
	Grant              viewGrantWire   `json:"grant"`
	Purpose            string          `json:"purpose"`
	Consumer           string          `json:"consumer"`
	MaxItems           uint32          `json:"max_items"`
	MaxBytes           uint32          `json:"max_bytes"`
	Query              json.RawMessage `json:"query"`
}

type SourcePreview struct {
	ConnectorID  string `json:"connector_id"`
	ConnectionID string `json:"connection_id"`
	Resource     string `json:"resource"`
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
	"resources": {}, "grant": {}, "purpose": {}, "consumer": {},
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
	if viewID == "calendar.timeline" {
		parsed, err := parseCalendarQuery(query, 128)
		if err != nil {
			return nil, 0, err
		}
		selected := service.Calendars[connectionID]
		if selected == nil {
			return nil, 0, errors.New("calendar unavailable")
		}
		view, err := selected.ReadCalendarView(ctx, time.UnixMilli(parsed.RangeStartUnixMS), time.UnixMilli(parsed.RangeEndUnixMS), parsed.Cursor, parsed.Limit)
		if err != nil {
			return nil, 0, err
		}
		return boundedCalendarView(view)
	}
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
	if input.ConnectorID == "" || !validConnectionID(input.ConnectionID) || input.Resource != remoteViewResource(viewID, input.ConnectionID) || len(input.Resource) > MaxResourceBytes || viewID == "calendar.timeline" && !connections.IsCalendarConnector(input.ConnectorID) {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	record, ok := service.Records[input.ConnectionID]
	if !ok || record.ConnectorID != input.ConnectorID || record.PersonID != principal.PersonID || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.Revision == 0 || record.Incarnation == "" || record.Epoch == 0 || record.ProviderIdentity == "" || record.IdentityUnverified {
		outcome = operation.Reject(operation.Conflict, "source_unavailable")
		return
	}
	sourceResources := []string{input.Resource}
	if viewID == "calendar.timeline" {
		var valid bool
		sourceResources, valid = calendarScopeResources(record)
		if !valid {
			outcome = operation.Reject(operation.Conflict, "source_unavailable")
			return
		}
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
		"source_resources":  sourceResources,
		"provider_identity": record.ProviderIdentity, "issued_at_unix_ms": time.Now().UnixMilli(),
	}
	descriptorBytes, err := json.Marshal(descriptor)
	if err != nil || len(descriptorBytes) > maxViewSourcePreviewProofBytes {
		outcome = operation.Reject(operation.Unavailable, "producer_unavailable")
		return
	}
	metadata["descriptor_b64url"] = base64.RawURLEncoding.EncodeToString(descriptorBytes)
	metadata["producer_signature"] = base64.RawURLEncoding.EncodeToString(service.Admissions.Producer().SignChallenge(descriptorBytes))
	metadata["expires_at_unix_ms"] = time.Now().Add(30 * time.Second).UnixMilli()
	metadata["connection_revision"] = record.Revision
	metadata["source_resources"] = sourceResources
	outcome = operation.Result{Category: operation.Ready, Value: metadata}
	return
}
func (service *SourceService) AdmitView(ctx context.Context, principal Principal, viewID string, input ViewAdmission) (outcome operation.Result) {

	if input.SchemaVersion != SchemaVersion || !validConnectionID(input.ConnectionID) || input.ConnectionRevision == 0 || len(input.Resources) != 1 || input.Resources[0] != remoteViewResource(viewID, input.ConnectionID) || strings.TrimSpace(input.Resources[0]) == "" || len(input.Resources[0]) > MaxResourceBytes || input.MaxItems == 0 || input.MaxItems > 128 || input.MaxBytes == 0 || input.MaxBytes > MaxStageBytesPerResult || input.Purpose == "" || input.Consumer == "" || len(input.Query) == 0 || len(input.Query) > maxCalendarQueryBytes {
		outcome = operation.Reject(operation.Invalid, "validation")
		return
	}
	if viewID == "mail.communication" {
		if input.ConnectorID != "gmail" && input.ConnectorID != "microsoft.mail" {
			outcome = operation.Reject(operation.Invalid, "validation")
			return
		}
	} else if viewID == "calendar.timeline" {
		if !connections.IsCalendarConnector(input.ConnectorID) {
			outcome = operation.Reject(operation.Invalid, "validation")
			return
		}
		if _, err := parseCalendarQuery(input.Query, input.MaxItems); err != nil {
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
	sourceResources := []string{input.Resources[0]}
	if viewID == "calendar.timeline" {
		var valid bool
		sourceResources, valid = calendarScopeResources(record)
		if !valid || service.Calendars[input.ConnectionID] == nil {
			outcome = operation.Reject(operation.Conflict, "connection_changed")
			return
		}
		if service.PreflightCalendarIdentity == nil || service.PreflightCalendarIdentity(ctx, record) != nil {
			outcome = operation.Reject(operation.Unavailable, "source_identity_unavailable")
			return
		}
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
	now := time.Now()
	clientAdmissions := 0
	for admissionID, admissionState := range service.Admissions.remoteView {
		if !admissionState.expires.After(now) {
			delete(service.Admissions.remoteView, admissionID)
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
	service.Admissions.remoteView[challenge.ID] = remoteViewAdmissionState{path: viewID, query: append([]byte(nil), input.Query...), principal: principal, connectorID: record.ConnectorID, connectionID: record.ConnectionID, connectionRev: input.ConnectionRevision, sourceResources: append([]string(nil), sourceResources...), providerIdentity: record.ProviderIdentity, expires: challenge.ExpiresAt}
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
	if !exists || record.ConnectorID != state.connectorID || record.PersonID != principal.PersonID || record.Revision != state.connectionRev || record.Incarnation != authorized.Source.Incarnation || record.Epoch != authorized.Source.Epoch || record.ProviderIdentity != state.providerIdentity || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.IdentityUnverified || authorized.Source.ConnectorID != state.connectorID || authorized.Source.ConnectionID != state.connectionID || authorized.Source.ExecutionOwner != service.ExecutionOwner() || len(authorized.Resources) != 1 || authorized.Resources[0] != remoteViewResource(viewID, state.connectionID) {
		authority.CancelAdmission(admissionID)
		service.deleteRemoteViewAdmission(admissionID)
		outcome = operation.Reject(operation.Conflict, "connection_changed")
		return
	}
	currentSourceResources := []string{authorized.Resources[0]}
	if viewID == "calendar.timeline" {
		var valid bool
		currentSourceResources, valid = calendarScopeResources(record)
		if !valid || service.Calendars[state.connectionID] == nil || service.PreflightCalendarIdentity == nil || service.PreflightCalendarIdentity(ctx, record) != nil {
			authority.CancelAdmission(admissionID)
			service.deleteRemoteViewAdmission(admissionID)
			outcome = operation.Reject(operation.Conflict, "connection_changed")
			return
		}
	}
	if !reflect.DeepEqual(currentSourceResources, state.sourceResources) {
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

var connectionIDPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$`)

func validConnectionID(value string) bool { return connectionIDPattern.MatchString(value) }
func StrictJSON(data []byte) bool         { return rejectDuplicateJSON(data) == nil }
