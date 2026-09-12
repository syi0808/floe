package console

import (
	"bytes"
	"crypto/ed25519"
	cryptorand "crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"

	"floe/server/internal/authorization"
)

func (console *Console) serveAuthority(writer http.ResponseWriter, request *http.Request, scope clientScope) {
	if request.Method == http.MethodGet && request.URL.Path == "/v1/authority/producer" {
		metadata, err := console.producerMetadata()
		if err != nil {
			failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
			return
		}
		reply(writer, http.StatusOK, metadata)
		return
	}
	if request.Method == http.MethodPost && request.URL.Path == "/v1/authority/calendar/source" {
		console.serveCalendarSourcePreview(writer, request, scope)
		return
	}
	engine := console.authorityEngine()
	if engine == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	principal := authorization.Principal{ClientID: scope.ClientID, PersonID: scope.PersonID, DeviceID: scope.DeviceID, Authenticated: true}
	switch {
	case request.Method == http.MethodPost && request.URL.Path == "/v1/authority/enrollment/begin":
		var input struct {
			KeyID     string `json:"key_id"`
			PublicKey string `json:"public_key"`
			Audience  string `json:"audience"`
		}
		if !strictAuthorityDecode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		publicKey, err := base64.RawURLEncoding.DecodeString(input.PublicKey)
		if err != nil || len(publicKey) != ed25519.PublicKeySize || base64.RawURLEncoding.EncodeToString(publicKey) != input.PublicKey {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		metadata, metadataError := console.producerMetadata()
		if metadataError != nil {
			failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
			return
		}
		audience, audienceOK := metadata["audience"].(string)
		if !audienceOK {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		enrollment, challenge, err := engine.BeginEnrollment(principal, input.KeyID, ed25519.PublicKey(publicKey), audience)
		if err != nil {
			failure(writer, http.StatusForbidden, "enrollment_denied")
			return
		}
		producerKeyID := metadata["key_id"]
		producerPublicKey := metadata["public_key"]
		producerFingerprint := metadata["fingerprint"]
		metadata["enrollment_id"] = enrollment.ID
		metadata["challenge_id"] = challenge.ID
		metadata["key_id"] = enrollment.KeyID
		metadata["fingerprint"] = enrollment.Fingerprint
		metadata["producer_key_id"] = producerKeyID
		metadata["producer_public_key"] = producerPublicKey
		metadata["producer_fingerprint"] = producerFingerprint
		delete(metadata, "public_key")
		metadata["challenge_b64url"] = challenge.BytesB64
		metadata["producer_signature"] = base64.RawURLEncoding.EncodeToString(console.producer.signChallenge(challenge.Bytes))
		metadata["expires"] = challenge.ExpiresAt
		reply(writer, http.StatusOK, metadata)
	case request.Method == http.MethodPost && request.URL.Path == "/v1/authority/enrollment/complete":
		var input struct {
			EnrollmentID string `json:"enrollment_id"`
			ChallengeID  string `json:"challenge_id"`
			KeyID        string `json:"key_id"`
			Signature    string `json:"signature"`
		}
		if !strictAuthorityDecode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if input.EnrollmentID == "" {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if err := engine.CompleteEnrollment(input.EnrollmentID, principal, authorization.Proof{ChallengeID: input.ChallengeID, KeyID: input.KeyID, Signature: input.Signature}); err != nil {
			failure(writer, http.StatusForbidden, "enrollment_denied")
			return
		}
		if status, statusErr := engine.EnrollmentStatus(input.EnrollmentID, principal); statusErr == nil && status.Active {
			reply(writer, http.StatusOK, map[string]string{"status": "active"})
			return
		}
		reply(writer, http.StatusOK, map[string]string{"status": "pending_admin"})
	case request.Method == http.MethodGet && strings.HasPrefix(request.URL.Path, "/v1/authority/enrollment/"):
		id := strings.TrimPrefix(request.URL.Path, "/v1/authority/enrollment/")
		status, err := engine.EnrollmentStatus(id, principal)
		if err != nil {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		reply(writer, http.StatusOK, map[string]any{"enrollment_id": status.ID, "key_id": status.KeyID, "fingerprint": status.Fingerprint, "local_confirmed": status.LocalConfirmed, "admin_approved": status.AdminApproved, "active": status.Active})
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
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
	engine := console.authorityEngine()
	if engine == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	switch request.URL.Path {
	case "/manage/api/authority/producer":
		if request.Method != http.MethodGet {
			failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
			return
		}
		metadata, err := console.producerMetadata()
		if err != nil {
			failure(writer, http.StatusServiceUnavailable, "producer_unavailable")
			return
		}
		reply(writer, http.StatusOK, metadata)
	case "/manage/api/authority/enrollments":
		if request.Method != http.MethodGet {
			failure(writer, http.StatusNotFound, "not_found")
			return
		}
		statuses := engine.PendingEnrollments()
		active := engine.ActiveIssuers()
		output := make([]map[string]any, 0, len(statuses))
		for _, status := range statuses {
			output = append(output, map[string]any{"enrollment_id": status.ID, "key_id": status.KeyID, "client_id": status.Principal.ClientID, "person_id": status.Principal.PersonID, "device_id": status.Principal.DeviceID, "fingerprint": status.Fingerprint, "local_confirmed": status.LocalConfirmed, "admin_approved": status.AdminApproved})
		}
		for _, status := range active {
			output = append(output, map[string]any{"enrollment_id": status.ID, "key_id": status.KeyID, "client_id": status.Principal.ClientID, "person_id": status.Principal.PersonID, "device_id": status.Principal.DeviceID, "fingerprint": status.Fingerprint, "active": true})
		}
		reply(writer, http.StatusOK, map[string]any{"enrollments": output})
	case "/manage/api/authority/approve", "/manage/api/authority/reject":
		if request.Method != http.MethodPost {
			failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
			return
		}
		var input struct {
			EnrollmentID string `json:"enrollment_id"`
			Fingerprint  string `json:"fingerprint"`
		}
		if !strictAuthorityDecode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		approved := request.URL.Path == "/manage/api/authority/approve"
		if err := engine.ApproveEnrollment(input.EnrollmentID, input.Fingerprint, approved); err != nil {
			failure(writer, http.StatusConflict, "enrollment_conflict")
			return
		}
		reply(writer, http.StatusOK, map[string]string{"status": "ok"})
	case "/manage/api/authority/revoke":
		if request.Method != http.MethodPost {
			failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
			return
		}
		var input struct {
			KeyID string `json:"key_id"`
		}
		if !strictAuthorityDecode(writer, request, &input) {
			failure(writer, http.StatusBadRequest, "validation")
			return
		}
		if err := engine.RevokeIssuer(input.KeyID); err != nil {
			failure(writer, http.StatusConflict, "revoke_failed")
			return
		}
		reply(writer, http.StatusOK, map[string]string{"status": "revoked"})
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}

func strictAuthorityDecode(writer http.ResponseWriter, request *http.Request, output any) bool {
	data, err := io.ReadAll(http.MaxBytesReader(writer, request.Body, authorization.MaxProofBytes))
	if err != nil || len(data) == 0 || !strictAuthorityJSON(data) {
		return false
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	return decoder.Decode(output) == nil && decoder.Decode(new(any)) == io.EOF
}

func strictAuthorityJSON(data []byte) bool {
	decoder := json.NewDecoder(bytes.NewReader(data))
	return walkAuthorityJSON(decoder, 0) == nil && decoder.Decode(new(any)) == io.EOF
}

func walkAuthorityJSON(decoder *json.Decoder, depth int) error {
	if depth > authorization.MaxJSONDepth {
		return errors.New("json depth")
	}
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok {
		return nil
	}
	if delimiter == '{' {
		seen := map[string]bool{}
		for decoder.More() {
			key, err := decoder.Token()
			if err != nil {
				return err
			}
			name, ok := key.(string)
			if !ok || name != strings.ToLower(name) || seen[name] {
				return errors.New("duplicate")
			}
			seen[name] = true
			if err := walkAuthorityJSON(decoder, depth+1); err != nil {
				return err
			}
		}
		_, err = decoder.Token()
		return err
	}
	if delimiter == '[' {
		for decoder.More() {
			if err := walkAuthorityJSON(decoder, depth+1); err != nil {
				return err
			}
		}
		_, err = decoder.Token()
		return err
	}
	return nil
}
