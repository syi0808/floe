// HTTP and admin-UI transport for the local server.

package httptransport

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
