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
	"reflect"
	"strings"
	"time"

	"floe/server/internal/authorization"
)

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
