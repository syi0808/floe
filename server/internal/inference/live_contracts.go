package inference

import (
	"encoding/json"
	"errors"
	"floe/server/internal/trust"
	"strings"
	"unicode"
	"unicode/utf8"
)

// LiveEvent is a normalized model observation, never a Manager command or audio
// frame. Its identifiers are meaningful only inside the owning live call.
type LiveEvent struct {
	Kind         string  `json:"kind"`
	Speaker      string  `json:"speaker,omitempty"`
	UtteranceID  string  `json:"utterance_id,omitempty"`
	Text         string  `json:"text,omitempty"`
	DelegationID string  `json:"delegation_id,omitempty"`
	UtteranceRef *string `json:"utterance_ref,omitempty"`
	InputHint    *string `json:"input_hint,omitempty"`
}

func (LiveEvent) String() string { return "LiveEvent[redacted]" }
func LiveIdentifier(value string) bool {
	return value != "" && len(value) <= 256 && utf8.ValidString(value) && strings.TrimSpace(value) == value && !strings.ContainsFunc(value, unicode.IsControl)
}
func LiveText(value string, final bool) bool {
	return len(value) <= 65536 && utf8.ValidString(value) && (!final || strings.TrimSpace(value) != "")
}
func (e LiveEvent) Validate() error {
	valid := false
	switch e.Kind {
	case "ready":
		valid = e.Speaker == "" && e.UtteranceID == "" && e.Text == "" && e.DelegationID == "" && e.UtteranceRef == nil && e.InputHint == nil
	case "transcript_delta":
		valid = (e.Speaker == "person" || e.Speaker == "assistant") && (e.UtteranceID == "" || LiveIdentifier(e.UtteranceID)) && LiveText(e.Text, false) && e.Text != "" && e.DelegationID == "" && e.UtteranceRef == nil && e.InputHint == nil
	case "person_utterance_final", "assistant_utterance_final":
		valid = e.Speaker == "" && LiveIdentifier(e.UtteranceID) && LiveText(e.Text, true) && e.DelegationID == "" && e.UtteranceRef == nil && e.InputHint == nil
	case "delegation_requested":
		valid = e.Speaker == "" && e.UtteranceID == "" && e.Text == "" && LiveIdentifier(e.DelegationID) && (e.UtteranceRef == nil || LiveIdentifier(*e.UtteranceRef)) && (e.InputHint == nil || LiveText(*e.InputHint, false))
	}
	if !valid {
		return errors.New("invalid live observation")
	}
	return nil
}
func (e LiveEvent) MarshalJSON() ([]byte, error) {
	if err := e.Validate(); err != nil {
		return nil, err
	}
	type wire LiveEvent
	return json.Marshal(wire(e))
}

// Logical identity is host correlation, not a provider idempotency key.
type LiveAppend struct {
	OperationID  string
	Kind         string
	DelegationID string
	Text         string
}

func (LiveAppend) String() string { return "LiveAppend[redacted]" }
func (a LiveAppend) Validate() error {
	if !trust.ValidID(a.OperationID) || !LiveText(a.Text, true) || (a.DelegationID != "" && !LiveIdentifier(a.DelegationID)) {
		return errors.New("invalid live append")
	}
	switch a.Kind {
	case "progress", "presentable_result", "instruction":
		return nil
	}
	return errors.New("invalid live append kind")
}
func (e LiveEvent) GoString() string  { return e.String() }
func (a LiveAppend) GoString() string { return a.String() }
