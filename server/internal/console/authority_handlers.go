package console

import (
	"bytes"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"

	"floe/server/internal/authorization"
)

func (console *Console) serveAuthority(writer http.ResponseWriter, request *http.Request, scope clientScope) {
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
		enrollment, challenge, err := engine.BeginEnrollment(principal, input.KeyID, ed25519.PublicKey(publicKey), input.Audience)
		if err != nil {
			failure(writer, http.StatusForbidden, "enrollment_denied")
			return
		}
		reply(writer, http.StatusOK, map[string]any{"enrollment_id": enrollment.ID, "challenge_id": challenge.ID, "key_id": enrollment.KeyID, "fingerprint": enrollment.Fingerprint, "challenge_b64url": challenge.BytesB64, "expires": challenge.ExpiresAt})
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

func (console *Console) manageAuthority(writer http.ResponseWriter, request *http.Request) {
	engine := console.authorityEngine()
	if engine == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	switch request.URL.Path {
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
			if !ok || seen[name] {
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
