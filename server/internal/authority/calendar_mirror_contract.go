package authority

import (
	"crypto/sha256"
	"encoding/hex"
	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/trust"
	viewcontracts "floe/server/internal/views/contracts"
	"reflect"
	"time"
)

type ProductCalendarChallenge struct {
	Operation       string                              `json:"operation"`
	Version         int                                 `json:"v"`
	ChallengeID     string                              `json:"challenge_id"`
	Nonce           string                              `json:"nonce"`
	KeyID           string                              `json:"key_id"`
	Claims          viewcontracts.ProductCalendarClaims `json:"claims"`
	AdmissionID     string                              `json:"admission_id,omitempty"`
	ResultSHA256    string                              `json:"result_sha256,omitempty"`
	IssuedAtUnixMS  int64                               `json:"issued_at_unix_ms"`
	ExpiresAtUnixMS int64                               `json:"expires_at_unix_ms"`
}
type productCalendarPolicy struct {
	claims  viewcontracts.ProductCalendarClaims
	expires time.Time
}

func (p productCalendarPolicy) requestBounds() sourcecontract.Bounds {
	return sourcecontract.Bounds{MaxItems: p.claims.Limits.MaxPageRecords, MaxBytes: p.claims.Limits.MaxPageBytes}
}
func (p productCalendarPolicy) clonePolicy() requestPolicy {
	p.claims = viewcontracts.CloneProductCalendarClaims(p.claims)
	return p
}
func (p productCalendarPolicy) validateSource(source sourcecontract.Snapshot) error {
	c := p.claims
	l := c.Limits
	if c.Purpose != "day_refresh" || c.ResultKind != "calendar.mirror" || !trust.ValidID(c.RefreshOperationID) || !trust.ValidID(c.ReadOperationID) || !trust.ValidID(c.PageID) || !trust.ValidID(c.EnrollmentID) || c.CredentialGeneration == 0 || c.Source.LocalRevision == 0 || !reflect.DeepEqual(c.Source, viewcontracts.CalendarSourceClaims(source, c.Source.LocalRevision)) || c.PersonID != source.PersonID || !reflect.DeepEqual(c.Resources, source.Resources) || len(c.Resources) == 0 || len(c.Resources) > 256 || l.MaxRecords == 0 || l.MaxRecords > 10000 || l.MaxBytes == 0 || l.MaxBytes > 4<<20 || l.MaxPageRecords == 0 || l.MaxPageRecords > 128 || l.MaxPageRecords > l.MaxRecords || l.MaxPageBytes == 0 || l.MaxPageBytes > 1<<20 || l.MaxPageBytes > l.MaxBytes {
		return ErrInvalid
	}
	digest := sha256.Sum256(c.Query)
	if hex.EncodeToString(digest[:]) != c.QuerySHA256 {
		return ErrInvalid
	}
	previous := ""
	for _, resource := range c.Resources {
		if validateBoundString(resource, 256) != nil || previous != "" && previous >= resource {
			return ErrInvalid
		}
		previous = resource
	}
	return nil
}
