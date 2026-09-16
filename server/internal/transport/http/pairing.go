// HTTP and admin-UI transport for the local server.

package httptransport

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"time"

	"floe/server/internal/authorization"
)

func (console *Console) managePairApprove(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		SchemaVersion int    `json:"schema_version"`
		PairingID     string `json:"pairing_id"`
		ID            string `json:"id"`
		Fingerprint   string `json:"issuer_fingerprint"`
	}
	if !decode(writer, request, &input) {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if input.SchemaVersion == 0 && input.PairingID == "" && input.ID != "" && input.Fingerprint == "" {
		console.manageLegacyPairApprove(writer, input.ID)
		return
	}
	if input.SchemaVersion != 1 || input.PairingID == "" || input.ID != "" {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if input.Fingerprint == "" {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}

	console.mu.Lock()
	pending := console.pair
	if pending == nil || pending.ID != input.PairingID {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "pairing_expired")
		return
	}
	if !pending.Expires.After(now()) {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "pairing_expired")
		return
	}
	if !pending.LocalConfirmed {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "pairing_not_confirmed")
		return
	}
	if pending.IssuerFingerprint != input.Fingerprint {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "fingerprint_mismatch")
		return
	}
	pendingCopy := *pending
	token := randomToken()
	console.mu.Unlock()

	engine := console.authorityEngine()
	if engine == nil {
		failure(writer, http.StatusServiceUnavailable, "authority_unavailable")
		return
	}
	err := engine.ApproveEnrollmentWithCommit(
		pendingCopy.enrollmentID,
		pendingCopy.IssuerFingerprint,
		func(record authorization.IssuerRecord) error {
			return console.commitPairingActivation(record, digest(token), pendingCopy)
		},
	)
	if err != nil {
		failure(writer, http.StatusConflict, "pairing_conflict")
		return
	}
	console.mu.Lock()
	if console.pair == nil || console.pair.ID != pendingCopy.ID {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "pairing_expired")
		return
	}
	console.pair.AdminApproved = true
	console.pair.status = "approved"
	console.pair.token = token
	console.mu.Unlock()
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "pairing_id": pendingCopy.ID, "status": "approved"})
}

func (console *Console) manageLegacyPairApprove(writer http.ResponseWriter, pairingID string) {
	console.mu.Lock()
	defer console.mu.Unlock()
	if console.pair == nil || console.pair.ID != pairingID || !console.pair.Expires.After(time.Now()) || console.pair.token != "" {
		failure(writer, http.StatusConflict, "pairing_expired")
		return
	}
	if console.pair.IssuerKeyID != "" {
		failure(writer, http.StatusConflict, "pairing_confirmation_required")
		return
	}
	next := cloneState(console.state)
	token := randomToken()
	next.Clients[pairingID] = pairedClient{ClientID: pairingID, TokenHash: digest(token), PersonID: console.pair.PersonID, DeviceID: console.pair.DeviceID}
	if err := console.save(next); err != nil {
		failure(writer, http.StatusInternalServerError, "save_failed")
		return
	}
	console.state, console.pair.token = next, token
	reply(writer, http.StatusOK, map[string]bool{"ok": true})
}

func (console *Console) managePairReject(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		SchemaVersion int    `json:"schema_version"`
		PairingID     string `json:"pairing_id"`
		ID            string `json:"id"`
	}
	if !decode(writer, request, &input) || input.PairingID != "" && input.ID != "" && input.PairingID != input.ID {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}
	if input.PairingID == "" {
		input.PairingID = input.ID
	}
	if input.PairingID == "" || input.SchemaVersion != 1 && input.ID == "" {
		failure(writer, http.StatusBadRequest, "validation")
		return
	}

	console.mu.Lock()
	if console.pair == nil || console.pair.ID != input.PairingID {
		console.mu.Unlock()
		failure(writer, http.StatusConflict, "pairing_expired")
		return
	}
	pending := *console.pair
	console.pair.status = "rejected"
	console.pair.LocalConfirmed = false
	console.pair.AdminApproved = false
	console.pair.token = ""
	console.mu.Unlock()

	if engine := console.authorityEngine(); engine != nil {
		_ = engine.ApproveEnrollment(pending.enrollmentID, pending.IssuerFingerprint, false)
	}
	reply(writer, http.StatusOK, map[string]any{"schema_version": 1, "pairing_id": pending.ID, "status": "rejected"})
}
