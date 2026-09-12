package console

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"

	"floe/server/internal/authorization"
)

func remoteViewResource(viewID, connectionID string) string {
	return viewID + ":" + connectionID
}

type remoteViewAdmissionState struct {
	path          string
	query         []byte
	principal     authorization.Principal
	connectorID   string
	connectionID  string
	connectionRev uint64
	expires       time.Time
}

type legacyCommunicationAdapter struct{ runtime ConnectorAuthRuntime }

func snapshotConnectionID(snapshot any) string {
	value, _, _, ok := connectionSnapshotMetadata(snapshot)
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

func (adapter legacyCommunicationAdapter) ConnectionSnapshot(context.Context) (any, error) {
	return adapter.runtime.ConnectionSnapshot()
}

func (adapter legacyCommunicationAdapter) ReadCommunicationView(_ context.Context, query string, cursor, limit int) (any, error) {
	return adapter.runtime.ReadCommunicationView(query, cursor, limit)
}

type remoteViewAdmissionWire struct {
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

func (console *Console) serveRemoteViewSourcePreview(writer http.ResponseWriter, request *http.Request, viewID string, scope clientScope, records map[string]connectionRecord) {
	if request.Method != http.MethodPost || viewID == "" {
		failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
		return
	}
	var input struct {
		ConnectorID  string `json:"connector_id"`
		ConnectionID string `json:"connection_id"`
		Resource     string `json:"resource"`
	}
	allowed := map[string]struct{}{"connector_id": {}, "connection_id": {}, "resource": {}}
	if !decodeCalendarEnvelope(writer, request, allowed, &input) || input.ConnectorID == "" || !validConnectionID(input.ConnectionID) || viewID != "calendar.timeline" && input.Resource != remoteViewResource(viewID, input.ConnectionID) || len(input.Resource) > authorization.MaxResourceBytes {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	record, ok := records[input.ConnectionID]
	if !ok || record.ConnectorID != input.ConnectorID || record.PersonID != scope.PersonID || record.Device != nil && record.Device.DeviceID != scope.DeviceID || record.Revision == 0 || record.Incarnation == "" || record.Epoch == 0 || record.ProviderIdentity == "" || record.IdentityUnverified {
		failure(writer, http.StatusConflict, "source_unavailable")
		return
	}
	principal := authorization.Principal{ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Authenticated: true}
	reference := authorization.SourceReference{ConnectorID: record.ConnectorID, ConnectionID: record.ConnectionID, ExecutionOwner: console.executionOwner(), Incarnation: record.Incarnation, Epoch: record.Epoch}
	if err := console.WithCurrentSource(principal, reference, func(authorization.SourceSnapshot) error { return nil }); err != nil {
		failure(writer, http.StatusConflict, "source_unavailable")
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
	challengeID, err := newConnectionID()
	if err != nil {
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	descriptor := map[string]any{
		"v": 1, "operation": "remote_view_source_preview", "challenge_id": challengeID,
		"nonce": randomToken(), "view_id": viewID, "person_id": scope.PersonID,
		"client_id": scope.ClientID, "device_id": scope.DeviceID, "audience": audience,
		"connector_id": record.ConnectorID, "connection_id": record.ConnectionID,
		"connection_revision": record.Revision, "execution_owner": console.executionOwner(),
		"incarnation": record.Incarnation, "epoch": record.Epoch, "resource": input.Resource,
		"provider_identity": record.ProviderIdentity, "issued_at_unix_ms": time.Now().UnixMilli(),
	}
	descriptorBytes, err := json.Marshal(descriptor)
	if err != nil {
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	metadata["descriptor_b64url"] = base64.RawURLEncoding.EncodeToString(descriptorBytes)
	metadata["producer_signature"] = base64.RawURLEncoding.EncodeToString(console.producer.signChallenge(descriptorBytes))
	metadata["expires_at_unix_ms"] = time.Now().Add(30 * time.Second).UnixMilli()
	metadata["connection_revision"] = record.Revision
	reply(writer, http.StatusOK, metadata)
}

func (console *Console) serveRemoteViewAuthority(
	writer http.ResponseWriter,
	request *http.Request,
	path string,
	principal authorization.Principal,
	connectionRecords map[string]connectionRecord,
	communication []CommunicationRuntime,
	work []WorkContextRuntime,
	logistics []LogisticsRuntime,
) {
	if request.Method != http.MethodPost {
		failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
		return
	}
	switch path {
	case "/v1/views/mail.communication/admit", "/v1/views/work.context/admit", "/v1/views/life.logistics/admit":
		console.serveRemoteViewAdmission(writer, request, path, principal, connectionRecords, communication, work, logistics)
	case "/v1/views/mail.communication/read", "/v1/views/work.context/read", "/v1/views/life.logistics/read":
		console.serveRemoteViewRead(writer, request, path, principal, connectionRecords, communication, work, logistics)
	case "/v1/views/mail.communication/release", "/v1/views/work.context/release", "/v1/views/life.logistics/release":
		console.serveRemoteViewRelease(writer, request, principal)
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}

func remoteViewRoute(path string) (string, string, bool) {
	switch path {
	case "/v1/views/mail.communication/admit", "/v1/views/mail.communication/read", "/v1/views/mail.communication/release":
		return "mail.communication", "gmail", true
	case "/v1/views/work.context/admit", "/v1/views/work.context/read", "/v1/views/work.context/release":
		return "work.context", "", true
	case "/v1/views/life.logistics/admit", "/v1/views/life.logistics/read", "/v1/views/life.logistics/release":
		return "life.logistics", "", true
	default:
		return "", "", false
	}
}

func (console *Console) serveRemoteViewAdmission(writer http.ResponseWriter, request *http.Request, path string, principal authorization.Principal, records map[string]connectionRecord, communication []CommunicationRuntime, work []WorkContextRuntime, logistics []LogisticsRuntime) {
	viewID, _, ok := remoteViewRoute(path)
	if !ok {
		failure(writer, http.StatusNotFound, "not_found")
		return
	}
	var input remoteViewAdmissionWire
	allowed := map[string]struct{}{"schema_version": {}, "connector_id": {}, "connection_id": {}, "connection_revision": {}, "resources": {}, "policy": {}, "grant": {}, "purpose": {}, "consumer": {}, "max_items": {}, "max_bytes": {}, "query": {}}
	if !decodeCalendarEnvelope(writer, request, allowed, &input) {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if input.SchemaVersion != authorization.SchemaVersion || !validConnectionID(input.ConnectionID) || input.ConnectionRevision == 0 || len(input.Resources) != 1 || input.Resources[0] != remoteViewResource(viewID, input.ConnectionID) || strings.TrimSpace(input.Resources[0]) == "" || len(input.Resources[0]) > authorization.MaxResourceBytes || input.MaxItems == 0 || input.MaxItems > 128 || input.MaxBytes == 0 || input.MaxBytes > authorization.MaxStageBytesPerResult || input.Purpose == "" || input.Consumer == "" || len(input.Query) == 0 || len(input.Query) > maxCalendarQueryBytes {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if viewID == "mail.communication" {
		if input.ConnectorID != "gmail" && input.ConnectorID != "microsoft.mail" {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
	} else if input.ConnectorID == "" {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	record, exists := records[input.ConnectionID]
	if !exists || record.ConnectorID != input.ConnectorID || record.PersonID != principal.PersonID || record.Revision != input.ConnectionRevision || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.Incarnation == "" || record.Epoch == 0 || record.ProviderIdentity == "" || record.IdentityUnverified {
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	queryDigest := sha256.Sum256(input.Query)
	authority := console.authorityEngine()
	if authority == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
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
	if console.remoteViewAdmissions == nil {
		console.remoteViewAdmissions = map[string]remoteViewAdmissionState{}
	}
	console.remoteViewAdmissions[challenge.ID] = remoteViewAdmissionState{path: viewID, query: append([]byte(nil), input.Query...), principal: principal, connectorID: record.ConnectorID, connectionID: record.ConnectionID, connectionRev: input.ConnectionRevision, expires: challenge.ExpiresAt}
	console.mu.Unlock()
	calendarChallengeReply(writer, http.StatusOK, "admission", challenge.ID, challenge.BytesB64, challenge.ExpiresAt, console.producer.signChallenge(challenge.Bytes), metadata)
	_ = communication
	_ = work
	_ = logistics
}

func (console *Console) serveRemoteViewRead(writer http.ResponseWriter, request *http.Request, path string, principal authorization.Principal, records map[string]connectionRecord, communication []CommunicationRuntime, work []WorkContextRuntime, logistics []LogisticsRuntime) {
	authority := console.authorityEngine()
	if authority == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	proof, ok := decodeCalendarProof(writer, request)
	if !ok {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	admissionID, authorized, err := authority.ClaimAdmission(principal, proof, console)
	if err != nil {
		failure(writer, http.StatusForbidden, "admission_denied")
		return
	}
	console.mu.Lock()
	state, found := console.remoteViewAdmissions[admissionID]
	console.mu.Unlock()
	viewID, _, _ := remoteViewRoute(path)
	if !found || state.path != viewID || state.principal != principal || !state.expires.After(time.Now()) || sha256.Sum256(state.query) != authorized.QueryDigest {
		authority.CancelAdmission(admissionID)
		failure(writer, http.StatusConflict, "admission_unavailable")
		return
	}
	record, exists := records[state.connectionID]
	if !exists || record.ConnectorID != state.connectorID || record.PersonID != principal.PersonID || record.Revision != state.connectionRev || record.Incarnation != authorized.Source.Incarnation || record.Epoch != authorized.Source.Epoch || record.Device != nil && record.Device.DeviceID != principal.DeviceID || record.IdentityUnverified {
		authority.CancelAdmission(admissionID)
		console.deleteRemoteViewAdmission(admissionID)
		failure(writer, http.StatusConflict, "connection_changed")
		return
	}
	result, items, err := console.readRemoteView(request.Context(), viewID, state.connectionID, state.connectorID, state.query, communication, work, logistics)
	if err != nil {
		authority.CancelAdmission(admissionID)
		console.deleteRemoteViewAdmission(admissionID)
		failure(writer, http.StatusServiceUnavailable, "view_unavailable")
		return
	}
	release, err := authority.StageResult(admissionID, principal, authorized, result, items)
	console.deleteRemoteViewAdmission(admissionID)
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
}

func (console *Console) readRemoteView(ctx context.Context, viewID, connectionID, connectorID string, query []byte, communication []CommunicationRuntime, work []WorkContextRuntime, logistics []LogisticsRuntime) ([]byte, uint32, error) {
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
			_, connector, _, ok := connectionSnapshotMetadata(snapshot)
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
			_, connector, _, ok := connectionSnapshotMetadata(snapshot)
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
			_, connector, _, ok := connectionSnapshotMetadata(snapshot)
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

func (console *Console) serveRemoteViewRelease(writer http.ResponseWriter, request *http.Request, principal authorization.Principal) {
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

func (console *Console) deleteRemoteViewAdmission(id string) {
	console.mu.Lock()
	delete(console.remoteViewAdmissions, id)
	console.mu.Unlock()
}
