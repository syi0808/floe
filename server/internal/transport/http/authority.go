package httptransport

import (
	"bytes"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	stdhttp "net/http"
	"strings"

	"floe/server/internal/authorization"
)

type EnrollmentService interface {
	BeginEnrollment(authorization.Principal, string, ed25519.PublicKey, string) (authorization.Enrollment, authorization.Challenge, error)
	CompleteEnrollment(string, authorization.Principal, authorization.Proof) error
	EnrollmentStatus(string, authorization.Principal) (authorization.EnrollmentStatus, error)
	PendingEnrollments() []authorization.EnrollmentStatus
	ActiveIssuers() []authorization.EnrollmentStatus
	ApproveEnrollment(string, string, bool) error
	RevokeIssuer(string) error
}

type AuthorityHandler struct {
	Service          EnrollmentService
	ProducerMetadata func() (map[string]any, error)
	Sign             func([]byte) []byte
}

func (handler AuthorityHandler) ServeClient(writer stdhttp.ResponseWriter, request *stdhttp.Request, principal authorization.Principal) bool {
	if request.URL.Path == "/v1/authority/producer" && request.Method == stdhttp.MethodGet {
		metadata, err := handler.metadata()
		if err != nil {
			failure(writer, stdhttp.StatusServiceUnavailable, "producer_unavailable")
			return true
		}
		reply(writer, stdhttp.StatusOK, metadata)
		return true
	}
	if !strings.HasPrefix(request.URL.Path, "/v1/authority/enrollment") {
		return false
	}
	if handler.Service == nil {
		failure(writer, stdhttp.StatusServiceUnavailable, "authority_unavailable")
		return true
	}
	switch {
	case request.Method == stdhttp.MethodPost && request.URL.Path == "/v1/authority/enrollment/begin":
		var input struct {
			KeyID     string `json:"key_id"`
			PublicKey string `json:"public_key"`
			Audience  string `json:"audience"`
		}
		if !strictDecode(writer, request, &input) {
			failure(writer, stdhttp.StatusBadRequest, "validation")
			return true
		}
		publicKey, err := base64.RawURLEncoding.DecodeString(input.PublicKey)
		if err != nil || len(publicKey) != ed25519.PublicKeySize || base64.RawURLEncoding.EncodeToString(publicKey) != input.PublicKey {
			failure(writer, stdhttp.StatusBadRequest, "validation")
			return true
		}
		metadata, err := handler.metadata()
		if err != nil {
			failure(writer, stdhttp.StatusServiceUnavailable, "producer_unavailable")
			return true
		}
		audience, ok := metadata["audience"].(string)
		if !ok {
			failure(writer, stdhttp.StatusBadRequest, "validation")
			return true
		}
		enrollment, challenge, err := handler.Service.BeginEnrollment(principal, input.KeyID, ed25519.PublicKey(publicKey), audience)
		if err != nil {
			failure(writer, stdhttp.StatusForbidden, "enrollment_denied")
			return true
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
		if handler.Sign == nil {
			failure(writer, stdhttp.StatusServiceUnavailable, "producer_unavailable")
			return true
		}
		metadata["producer_signature"] = base64.RawURLEncoding.EncodeToString(handler.Sign(challenge.Bytes))
		metadata["expires"] = challenge.ExpiresAt
		reply(writer, stdhttp.StatusOK, metadata)
	case request.Method == stdhttp.MethodPost && request.URL.Path == "/v1/authority/enrollment/complete":
		var input struct {
			EnrollmentID string `json:"enrollment_id"`
			ChallengeID  string `json:"challenge_id"`
			KeyID        string `json:"key_id"`
			Signature    string `json:"signature"`
		}
		if !strictDecode(writer, request, &input) {
			failure(writer, stdhttp.StatusBadRequest, "validation")
			return true
		}
		if input.EnrollmentID == "" {
			failure(writer, stdhttp.StatusBadRequest, "validation")
			return true
		}
		if err := handler.Service.CompleteEnrollment(input.EnrollmentID, principal, authorization.Proof{ChallengeID: input.ChallengeID, KeyID: input.KeyID, Signature: input.Signature}); err != nil {
			failure(writer, stdhttp.StatusForbidden, "enrollment_denied")
			return true
		}
		if status, statusErr := handler.Service.EnrollmentStatus(input.EnrollmentID, principal); statusErr == nil && status.Active {
			reply(writer, stdhttp.StatusOK, map[string]string{"status": "active"})
			return true
		}
		reply(writer, stdhttp.StatusOK, map[string]string{"status": "pending_admin"})
	case request.Method == stdhttp.MethodGet && strings.HasPrefix(request.URL.Path, "/v1/authority/enrollment/"):
		id := strings.TrimPrefix(request.URL.Path, "/v1/authority/enrollment/")
		status, err := handler.Service.EnrollmentStatus(id, principal)
		if err != nil {
			failure(writer, stdhttp.StatusNotFound, "not_found")
			return true
		}
		reply(writer, stdhttp.StatusOK, map[string]any{"enrollment_id": status.ID, "key_id": status.KeyID, "fingerprint": status.Fingerprint, "local_confirmed": status.LocalConfirmed, "admin_approved": status.AdminApproved, "active": status.Active})
	default:
		failure(writer, stdhttp.StatusNotFound, "not_found")
	}
	return true
}

func (handler AuthorityHandler) ServeAdmin(writer stdhttp.ResponseWriter, request *stdhttp.Request) bool {
	if !strings.HasPrefix(request.URL.Path, "/manage/api/authority/") {
		return false
	}
	if handler.Service == nil {
		failure(writer, stdhttp.StatusServiceUnavailable, "authority_unavailable")
		return true
	}
	switch request.URL.Path {
	case "/manage/api/authority/producer":
		if request.Method != stdhttp.MethodGet {
			failure(writer, stdhttp.StatusMethodNotAllowed, "method_not_allowed")
			return true
		}
		metadata, err := handler.metadata()
		if err != nil {
			failure(writer, stdhttp.StatusServiceUnavailable, "producer_unavailable")
			return true
		}
		reply(writer, stdhttp.StatusOK, metadata)
	case "/manage/api/authority/enrollments":
		if request.Method != stdhttp.MethodGet {
			failure(writer, stdhttp.StatusNotFound, "not_found")
			return true
		}
		pending := handler.Service.PendingEnrollments()
		active := handler.Service.ActiveIssuers()
		output := make([]map[string]any, 0, len(pending)+len(active))
		for _, status := range pending {
			output = append(output, map[string]any{"enrollment_id": status.ID, "key_id": status.KeyID, "client_id": status.Principal.ClientID, "person_id": status.Principal.PersonID, "device_id": status.Principal.DeviceID, "fingerprint": status.Fingerprint, "local_confirmed": status.LocalConfirmed, "admin_approved": status.AdminApproved})
		}
		for _, status := range active {
			output = append(output, map[string]any{"enrollment_id": status.ID, "key_id": status.KeyID, "client_id": status.Principal.ClientID, "person_id": status.Principal.PersonID, "device_id": status.Principal.DeviceID, "fingerprint": status.Fingerprint, "active": true})
		}
		reply(writer, stdhttp.StatusOK, map[string]any{"enrollments": output})
	case "/manage/api/authority/approve", "/manage/api/authority/reject":
		if request.Method != stdhttp.MethodPost {
			failure(writer, stdhttp.StatusMethodNotAllowed, "method_not_allowed")
			return true
		}
		var input struct {
			EnrollmentID string `json:"enrollment_id"`
			Fingerprint  string `json:"fingerprint"`
		}
		if !strictDecode(writer, request, &input) {
			failure(writer, stdhttp.StatusBadRequest, "validation")
			return true
		}
		if err := handler.Service.ApproveEnrollment(input.EnrollmentID, input.Fingerprint, request.URL.Path == "/manage/api/authority/approve"); err != nil {
			failure(writer, stdhttp.StatusConflict, "enrollment_conflict")
			return true
		}
		reply(writer, stdhttp.StatusOK, map[string]string{"status": "ok"})
	case "/manage/api/authority/revoke":
		if request.Method != stdhttp.MethodPost {
			failure(writer, stdhttp.StatusMethodNotAllowed, "method_not_allowed")
			return true
		}
		var input struct {
			KeyID string `json:"key_id"`
		}
		if !strictDecode(writer, request, &input) {
			failure(writer, stdhttp.StatusBadRequest, "validation")
			return true
		}
		if err := handler.Service.RevokeIssuer(input.KeyID); err != nil {
			failure(writer, stdhttp.StatusConflict, "revoke_failed")
			return true
		}
		reply(writer, stdhttp.StatusOK, map[string]string{"status": "revoked"})
	default:
		failure(writer, stdhttp.StatusNotFound, "not_found")
	}
	return true
}

func (handler AuthorityHandler) metadata() (map[string]any, error) {
	if handler.ProducerMetadata == nil {
		return nil, errors.New("producer metadata unavailable")
	}
	return handler.ProducerMetadata()
}

func reply(writer stdhttp.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func failure(writer stdhttp.ResponseWriter, status int, code string) {
	reply(writer, status, map[string]any{"error": map[string]string{"code": code}})
}

func strictDecode(writer stdhttp.ResponseWriter, request *stdhttp.Request, output any) bool {
	data, err := io.ReadAll(stdhttp.MaxBytesReader(writer, request.Body, authorization.MaxProofBytes))
	if err != nil || len(data) == 0 || !StrictJSON(data) {
		return false
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	return decoder.Decode(output) == nil && decoder.Decode(new(any)) == io.EOF
}

func StrictJSON(data []byte) bool {
	decoder := json.NewDecoder(bytes.NewReader(data))
	return walkJSON(decoder, 0) == nil && decoder.Decode(new(any)) == io.EOF
}

func walkJSON(decoder *json.Decoder, depth int) error {
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
				return errors.New("duplicate or non-lowercase key")
			}
			seen[name] = true
			if err := walkJSON(decoder, depth+1); err != nil {
				return err
			}
		}
		_, err = decoder.Token()
		return err
	}
	if delimiter == '[' {
		for decoder.More() {
			if err := walkJSON(decoder, depth+1); err != nil {
				return err
			}
		}
		_, err = decoder.Token()
		return err
	}
	return nil
}
