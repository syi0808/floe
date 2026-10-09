package authority

import (
	"floe/server/internal/contracts/source"
	"floe/server/internal/trust"
)

// SourceFence is the Authority-owned capability for checking the live source
// identity while consuming signed admissions and releases.
type SourceFence interface {
	WithCurrentSource(trust.Principal, source.Snapshot, func(source.Snapshot) error) error
}

type ProducerTrust interface {
	ProducerMetadata() (trust.ProducerMetadata, error)
	SignProducerChallenge([]byte) ([]byte, error)
}
