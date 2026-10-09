package views

import (
	"encoding/json"

	viewcontracts "floe/server/internal/views/contracts"
)

type CalendarProductPage struct {
	SchemaVersion      int                                       `json:"schema_version"`
	ResultKind         string                                    `json:"result_kind"`
	RefreshOperationID string                                    `json:"refresh_operation_id"`
	ReadOperationID    string                                    `json:"read_operation_id"`
	PageID             string                                    `json:"page_id"`
	PersonID           string                                    `json:"person_id"`
	DeviceID           string                                    `json:"device_id"`
	Source             viewcontracts.ProductCalendarSourceClaims `json:"source"`
	CalendarID         string                                    `json:"calendar_id"`
	RangeStartUnixMS   int64                                     `json:"range_start_unix_ms"`
	RangeEndUnixMS     int64                                     `json:"range_end_unix_ms"`
	ObservedAtUnixMS   int64                                     `json:"observed_at_unix_ms"`
	ExpiresAtUnixMS    int64                                     `json:"expires_at_unix_ms"`
	Outcome            calendarPageOutcome                       `json:"outcome"`
}

type calendarPageOutcome struct {
	state          string
	records        []viewcontracts.CalendarRecord
	cursor, reason string
}

func (outcome calendarPageOutcome) MarshalJSON() ([]byte, error) {
	switch outcome.state {
	case "complete":
		if outcome.records == nil || outcome.cursor != "" || outcome.reason != "" {
			return nil, ErrInvalid
		}
		return json.Marshal(struct {
			State   string                         `json:"state"`
			Records []viewcontracts.CalendarRecord `json:"records"`
		}{outcome.state, outcome.records})
	case "more":
		if outcome.records == nil || outcome.cursor == "" || outcome.reason != "" {
			return nil, ErrInvalid
		}
		return json.Marshal(struct {
			State   string                         `json:"state"`
			Records []viewcontracts.CalendarRecord `json:"records"`
			Cursor  string                         `json:"cursor"`
		}{outcome.state, outcome.records, outcome.cursor})
	case "failed":
		if outcome.records != nil || outcome.cursor != "" || !calendarFailure(outcome.reason) {
			return nil, ErrInvalid
		}
		return json.Marshal(struct {
			State  string `json:"state"`
			Reason string `json:"reason"`
		}{outcome.state, outcome.reason})
	default:
		return nil, ErrInvalid
	}
}

func calendarFailure(reason string) bool {
	switch reason {
	case "permission_denied", "calendar_unavailable", "provider_unavailable", "source_changed", "source_fenced", "vault_locked", "budget_exceeded", "deadline_exceeded", "cancelled":
		return true
	default:
		return false
	}
}
