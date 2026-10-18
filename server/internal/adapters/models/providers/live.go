package providers

// The provider adapter binds the configured model and existing Codex credential
// owner to Inference's long-lived port. No credential or provider call ID escapes.
import (
	"context"
	codexauth "floe/server/internal/adapters/models/codex"
	"floe/server/internal/inference"
	"time"
)

type codexLiveClient interface {
	PrepareLive(context.Context, string) (*codexauth.LivePrepared, error)
}

func (p *provider) LiveProfile() (inference.LiveProfile, bool) {
	_, available := p.codex.(codexLiveClient)
	if !available || p.target.BudgetOverride != nil || p.target.Provider != "codex_oauth" || p.target.Model != "gpt-live-1-codex" || p.target.BaseURL != "https://chatgpt.com/backend-api/codex" {
		return inference.LiveProfile{}, false
	}
	return inference.LiveProfile{
		ModelIdentity: p.ModelIdentity(),
		Capabilities:  []string{"duplex_audio", "final_input_transcript", "final_output_transcript", "client_delegation", "context_append"},
		// Enforced host ceilings, not undocumented provider capacity claims.
		Limits:         inference.LiveTransportLimits{InitialContextBytes: 2 * 1024 * 1024, AppendBytes: 65536, LocalSessionDurationMS: 60 * 60 * 1000},
		ProtocolSource: "openai/codex@0df4aa67781ae45a3c97befa4db48b734afe46d9",
		Experimental:   true,
	}, true
}
func (p *provider) PrepareLiveTransport(ctx context.Context, expectedIdentity string) (inference.PreparedLiveTransport, error) {
	profile, ok := p.LiveProfile()
	if !ok || profile.Validate() != nil || expectedIdentity == "" || p.ReplayIdentity() != expectedIdentity {
		return nil, inference.Failure{Code: inference.CapabilityChanged}
	}
	if err := p.Ready(ctx); err != nil {
		return nil, err
	}
	prepared, err := p.codex.(codexLiveClient).PrepareLive(ctx, p.codex.ReplayIdentity())
	if err != nil {
		return nil, err
	}
	if prepared == nil {
		return nil, inference.Failure{Code: inference.ModelUnavailable}
	}
	if p.ReplayIdentity() != expectedIdentity {
		return nil, inference.Failure{Code: inference.CapabilityChanged}
	}
	return &preparedLive{transport: prepared, model: p.target.Model, limits: profile.Limits}, nil
}

type preparedLive struct {
	transport *codexauth.LivePrepared
	model     string
	limits    inference.LiveTransportLimits
}

func (*preparedLive) String() string     { return "PreparedLiveTransport[redacted]" }
func (p *preparedLive) GoString() string { return p.String() }
func (p *preparedLive) request(in inference.LiveStartInput) codexauth.LiveStart {
	return codexauth.LiveStart{Model: p.model, Instructions: in.Instructions, InitialContext: in.InitialContext, SDP: in.Offer}
}
func (p *preparedLive) ValidateStart(in inference.LiveStartInput) error {
	if p.limits.Validate() != nil || uint64(len(in.InitialContext)) > p.limits.InitialContextBytes {
		return inference.Failure{Code: inference.RequestRejected}
	}
	return p.transport.ValidateStart(p.request(in))
}
func (p *preparedLive) Start(ctx context.Context, in inference.LiveStartInput) (inference.LiveTransport, error) {
	if err := p.ValidateStart(in); err != nil {
		return nil, err
	}
	deadline, bounded := ctx.Deadline()
	if !bounded || !deadline.After(time.Now()) || ctx.Err() != nil {
		return nil, inference.Failure{Code: inference.RequestRejected}
	}
	lifetime, cancel := context.WithTimeout(ctx, time.Duration(p.limits.LocalSessionDurationMS)*time.Millisecond)
	call, err := p.transport.Start(lifetime, p.request(in))
	if err != nil {
		cancel()
		if call != nil {
			call.CloseLocal()
		}
		return nil, err
	}
	if err = call.Attach(); err != nil {
		cancel()
		call.CloseLocal()
		return nil, err
	}
	go func() { <-call.Done(); cancel() }()
	return &liveTransport{call: call, limits: p.limits}, nil
}

type liveTransport struct {
	call   *codexauth.LiveCall
	limits inference.LiveTransportLimits
}

func (*liveTransport) String() string            { return "LiveTransport[redacted]" }
func (c *liveTransport) Answer() (string, error) { return c.call.AnswerSDP() }
func (c *liveTransport) Observe(ctx context.Context) (inference.LiveDelivery, error) {
	d, err := c.call.Observe(ctx)
	if err != nil {
		return inference.LiveDelivery{}, err
	}
	return inference.LiveDelivery{Sequence: d.Sequence, Event: d.Event}, nil
}
func (c *liveTransport) Acknowledge(sequence uint64) error { return c.call.Acknowledge(sequence) }
func (c *liveTransport) PrepareAppend(in inference.LiveAppend) (inference.PreparedLiveWrite, error) {
	if uint64(len(in.Text)) > c.limits.AppendBytes {
		return nil, inference.Failure{Code: inference.RequestRejected}
	}
	write, err := c.call.PrepareAppend(in)
	if err != nil {
		return nil, err
	}
	return &liveWrite{write}, nil
}
func (c *liveTransport) RequestClose(ctx context.Context) (bool, error) {
	return c.call.RequestClose(ctx)
}
func (c *liveTransport) Fence()                { c.call.CloseLocal() }
func (c *liveTransport) Done() <-chan struct{} { return c.call.Done() }

type liveWrite struct{ write *codexauth.LivePreparedAppend }

func (*liveWrite) String() string     { return "LiveWrite[redacted]" }
func (w *liveWrite) GoString() string { return w.String() }
func (w *liveWrite) Send(ctx context.Context) (inference.LiveWriteObservation, error) {
	r, err := w.write.Send(ctx)
	return inference.LiveWriteObservation{CompletedFrames: r.CompletedFrames, WriteComplete: r.WriteComplete}, err
}

func (c *liveTransport) GoString() string { return c.String() }
