package console

import (
	"bytes"
	cryptorand "crypto/rand"
	"encoding/base64"
	"encoding/json"
	"io"
	"net/http"
	"time"

	"floe/server/internal/authorization"
	transporthttp "floe/server/internal/transport/http"
)

func (console *Console) serveAuthority(writer http.ResponseWriter, request *http.Request, scope clientScope) {
	principal := authorization.Principal{ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Authenticated: true}
	engine := console.authorityEngine()
	authorityTransport := transporthttp.AuthorityHandler{ProducerMetadata: console.producerMetadata}
	if engine != nil {
		authorityTransport.Service = engine
	}
	if console.producer != nil {
		authorityTransport.Sign = console.producer.signChallenge
	}
	if authorityTransport.ServeClient(writer, request, principal) {
		return
	}
	if request.Method == http.MethodPost && request.URL.Path == "/v1/authority/calendar/source" {
		console.serveCalendarSourcePreview(writer, request, scope)
		return
	}
	if engine == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	failure(writer, http.StatusNotFound, "not_found")
}

func (console *Console) serveCalendarSourcePreview(writer http.ResponseWriter, request *http.Request, scope clientScope) {
	var input struct {
		ConnectorID  string `json:"connector_id"`
		ConnectionID string `json:"connection_id"`
		Resource     string `json:"resource"`
	}
	if !strictAuthorityDecode(writer, request, &input) || input.ConnectorID == "" || input.ConnectionID == "" || input.Resource == "" || len(input.Resource) > authorization.MaxResourceBytes {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	console.mu.Lock()
	record, ok := console.state.Connections[input.ConnectionID]
	if ok {
		record.Scope = cloneConnectorScope(record.Scope)
		if record.Device != nil {
			record.Device = &deviceBinding{DeviceID: record.Device.DeviceID}
		}
	}
	console.mu.Unlock()
	if !ok || record.PersonID != scope.PersonID || record.ConnectorID != input.ConnectorID || record.IdentityUnverified || record.ProviderIdentity == "" || record.Epoch == 0 || record.Incarnation == "" || record.Device != nil && record.Device.DeviceID != scope.DeviceID || record.Scope == nil || record.Scope["calendar_id"] != input.Resource {
		failure(writer, http.StatusConflict, "source_unavailable")
		return
	}
	metadata, err := console.producerMetadata()
	if err != nil {
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	nonce := make([]byte, 32)
	if _, err := cryptorand.Read(nonce); err != nil {
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	challengeID, err := newConnectionID()
	if err != nil {
		failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
		return
	}
	audience, _ := metadata["audience"].(string)
	executionOwner := console.executionOwner()
	principal := authorization.Principal{ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Authenticated: true}
	reference := authorization.SourceReference{ConnectorID: record.ConnectorID, ConnectionID: record.ConnectionID, ExecutionOwner: executionOwner, Incarnation: record.Incarnation, Epoch: record.Epoch}
	var descriptorBytes []byte
	var producerSignature []byte
	err = console.WithCurrentSource(principal, reference, func(authorization.SourceSnapshot) error {
		descriptor := map[string]any{
			"v": 1, "operation": "calendar_source_preview", "challenge_id": challengeID,
			"nonce": base64.RawURLEncoding.EncodeToString(nonce), "person_id": scope.PersonID,
			"client_id": scope.ClientID, "device_id": scope.DeviceID, "audience": audience,
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
		producerSignature = console.producer.signChallenge(descriptorBytes)
		return nil
	})
	if err != nil {
		failure(writer, http.StatusConflict, "source_unavailable")
		return
	}
	metadata["descriptor_b64url"] = base64.RawURLEncoding.EncodeToString(descriptorBytes)
	metadata["producer_signature"] = base64.RawURLEncoding.EncodeToString(producerSignature)
	metadata["expires_at_unix_ms"] = time.Now().Add(30 * time.Second).UnixMilli()
	reply(writer, http.StatusOK, metadata)
}

func (console *Console) manageAuthority(writer http.ResponseWriter, request *http.Request) {
	authorityTransport := transporthttp.AuthorityHandler{ProducerMetadata: console.producerMetadata}
	if engine := console.authorityEngine(); engine != nil {
		authorityTransport.Service = engine
	}
	if console.producer != nil {
		authorityTransport.Sign = console.producer.signChallenge
	}
	authorityTransport.ServeAdmin(writer, request)
}

func strictAuthorityDecode(writer http.ResponseWriter, request *http.Request, output any) bool {
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, authorization.MaxProofBytes))
	if err != nil || len(data) == 0 || !transporthttp.StrictJSON(data) {
		return false
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	return decoder.Decode(output) == nil && decoder.Decode(new(any)) == io.EOF
}

func strictAuthorityJSON(data []byte) bool {
	return transporthttp.StrictJSON(data)
}
