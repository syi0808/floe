package inference

import (
	"context"
	"errors"
	"math"
	"time"
)

// Live capabilities are a separate long-lived transport contract, never chat,
// tool-proposal or structured-generation capability aliases.
var liveCapabilityOrder = []string{"duplex_audio", "final_input_transcript", "final_output_transcript", "client_delegation", "context_append"}

type LiveTransportLimits struct {
	InitialContextBytes    uint64 `json:"initial_context_bytes"`
	AppendBytes            uint64 `json:"append_bytes"`
	LocalSessionDurationMS uint64 `json:"local_session_duration_ms"`
}

func (l LiveTransportLimits) Validate() error {
	if l.LocalSessionDurationMS > uint64(math.MaxInt64/int64(time.Millisecond)) {
		return errors.New("live duration cannot be represented")
	}
	for _, n := range []uint64{l.InitialContextBytes, l.AppendBytes, l.LocalSessionDurationMS} {
		if n == 0 || n > 9007199254740991 {
			return errors.New("invalid live transport limits")
		}
	}
	return nil
}

// Profile describes source-verified adapter support and explicit host limits,
// not account quota, a provider context window, billing or remote expiry.
type LiveProfile struct {
	ModelIdentity  ModelIdentity
	Capabilities   []string
	Limits         LiveTransportLimits
	ProtocolSource string
	Experimental   bool
}

func (p LiveProfile) Validate() error {
	if !validModelIdentity(p.ModelIdentity) || p.Limits.Validate() != nil || !LiveIdentifier(p.ProtocolSource) || len(p.Capabilities) == 0 || len(p.Capabilities) > len(liveCapabilityOrder) {
		return errors.New("invalid live profile")
	}
	last := -1
	for _, capability := range p.Capabilities {
		found := -1
		for i, known := range liveCapabilityOrder {
			if capability == known {
				found = i
				break
			}
		}
		if found <= last {
			return errors.New("invalid live capability order")
		}
		last = found
	}
	return nil
}
func CloneLiveProfile(p LiveProfile) LiveProfile {
	p.Capabilities = append([]string(nil), p.Capabilities...)
	return p
}

// Model selection and client authorization still belong to Inference. Accounts
// expose an optional live capability on the same configured target/credential.
type LiveModelAccount interface {
	ModelAccount
	LiveProfile() (LiveProfile, bool)
	PrepareLiveTransport(context.Context, string) (PreparedLiveTransport, error)
}
type LiveStartInput struct{ Instructions, InitialContext, Offer string }

func (LiveStartInput) String() string     { return "LiveStartInput[redacted]" }
func (v LiveStartInput) GoString() string { return v.String() }

type PreparedLiveTransport interface {
	// Pure local whole-request preflight before owner custody/provider I/O.
	ValidateStart(LiveStartInput) error
	// One-shot, under a bounded owner lifetime, not an HTTP observer context.
	Start(context.Context, LiveStartInput) (LiveTransport, error)
}
type LiveDelivery struct {
	Sequence uint64
	Event    LiveEvent
}

func (d LiveDelivery) Validate() error {
	if d.Sequence == 0 || d.Sequence > 9007199254740991 {
		return errors.New("invalid live delivery")
	}
	return d.Event.Validate()
}
func (LiveDelivery) String() string     { return "LiveDelivery[redacted]" }
func (d LiveDelivery) GoString() string { return d.String() }

type LiveWriteObservation struct {
	CompletedFrames int
	WriteComplete   bool
}
type PreparedLiveWrite interface {
	Send(context.Context) (LiveWriteObservation, error)
}
type LiveTransport interface {
	Answer() (string, error)
	Observe(context.Context) (LiveDelivery, error)
	Acknowledge(uint64) error
	PrepareAppend(LiveAppend) (PreparedLiveWrite, error)
	RequestClose(context.Context) (bool, error)
	Fence()
	Done() <-chan struct{}
}
