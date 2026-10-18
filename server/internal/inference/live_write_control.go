package inference

import (
	"context"
	"encoding/json"
	"floe/server/internal/trust"
	"time"
)

const maxLiveWriteRecords = 256
const liveWriteTimeout = 10 * time.Second

// Process-local transport evidence complements, but never replaces, the Voice
// owner's durable logical intent/receipt. Unknown prior-process tickets fail.
type LiveWriteStatus struct {
	OperationID     string
	Phase           string // pending, written, not_sent, uncertain
	Attempted       bool
	CompletedFrames int
	Failure         FailureCode
}

func (LiveWriteStatus) String() string     { return "LiveWriteStatus[redacted]" }
func (v LiveWriteStatus) GoString() string { return v.String() }

type liveWriteEntry struct {
	fingerprint string
	status      LiveWriteStatus
}

func writeFingerprint(request LiveAppend) (string, error) {
	data, err := json.Marshal(request)
	if err != nil {
		return "", Failure{Code: Validation}
	}
	return trust.Digest(string(data)), nil
}

// AppendLiveCall admits one exact write, independent of HTTP observation. The
// same operation/payload returns existing evidence; changed payload is rejected.
// A finite identity set prevents old IDs being forgotten/reused during a call.
func (s *Service) AppendLiveCall(ctx context.Context, p trust.Principal, id string, in LiveAppend) (LiveWriteStatus, error) {
	r := s.live
	e, err := r.lookup(p, id)
	if err != nil {
		return LiveWriteStatus{}, err
	}
	if in.Validate() != nil || in.Kind == "instruction" || uint64(len(in.Text)) > e.selected.profile.Limits.AppendBytes {
		return LiveWriteStatus{}, Failure{Code: Validation}
	}
	fingerprint, err := writeFingerprint(in)
	if err != nil {
		return LiveWriteStatus{}, err
	}
	if err = r.source(ctx, e, false); err != nil {
		return LiveWriteStatus{}, err
	}
	if err = ctx.Err(); err != nil {
		return LiveWriteStatus{}, err
	}
	r.mu.Lock()
	e.mu.Lock()
	if r.closed || e.retired {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveWriteStatus{}, Failure{Code: NotFound}
	}
	if previous := e.writes[in.OperationID]; previous != nil {
		if previous.fingerprint != fingerprint {
			e.mu.Unlock()
			r.mu.Unlock()
			return LiveWriteStatus{}, Failure{Code: RequestRejected}
		}
		out := previous.status
		e.mu.Unlock()
		r.mu.Unlock()
		return out, nil
	}
	if e.phase != "ready" || e.closeWrite != nil || e.transport == nil || e.controlCtx.Err() != nil {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveWriteStatus{}, Failure{Code: ModelUnavailable}
	}
	if len(e.writes) >= maxLiveWriteRecords {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveWriteStatus{}, Failure{Code: QuotaExceeded}
	}
	if e.activeWrite != "" {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveWriteStatus{}, Failure{Code: ModelBusy}
	}
	// The port is pure bounded codec/current-call validation, with no network I/O.
	prepared, err := e.transport.PrepareAppend(in)
	if err != nil || prepared == nil {
		e.mu.Unlock()
		r.mu.Unlock()
		if err == nil {
			err = Failure{Code: InvalidOutput}
		}
		return LiveWriteStatus{}, err
	}
	record := &liveWriteEntry{fingerprint: fingerprint, status: LiveWriteStatus{OperationID: in.OperationID, Phase: "pending"}}
	e.writes[in.OperationID] = record
	e.activeWrite = in.OperationID
	e.workers++
	r.wg.Add(1)
	out := record.status
	e.mu.Unlock()
	r.mu.Unlock()
	go r.sendAppend(e, record, prepared)
	return out, nil
}
func (r *liveRegistry) sendAppend(e *liveEntry, record *liveWriteEntry, prepared PreparedLiveWrite) {
	defer r.workerDone(e)
	ctx, cancel := context.WithTimeout(e.transportCtx, liveWriteTimeout)
	defer cancel()
	if err := r.source(ctx, e, false); err != nil {
		r.settleWrite(e, record, "not_sent", false, 0, liveCause(err))
		return
	}
	e.mu.Lock()
	allowed := !e.retired && e.phase == "ready" && e.closeWrite == nil && ctx.Err() == nil
	if allowed {
		record.status.Attempted = true
	}
	e.mu.Unlock()
	if !allowed {
		r.settleWrite(e, record, "not_sent", false, 0, ModelUnavailable)
		return
	}
	observed, err := prepared.Send(ctx)
	// Once Send is entered, any failed/lost evidence is conservative uncertainty.
	// Even a zero-frame error cannot authorize retrying the same logical write.
	if err != nil || !observed.WriteComplete || observed.CompletedFrames <= 0 || observed.CompletedFrames > 1024 {
		issue := liveCause(err)
		if err == nil {
			issue = InvalidOutput
		}
		frames := observed.CompletedFrames
		if frames < 0 || frames > 1024 {
			frames = 0
		}
		// Fence admission before clearing the active-write slot.
		r.fail(e, issue)
		r.settleWrite(e, record, "uncertain", true, frames, issue)
		return
	}
	r.settleWrite(e, record, "written", true, observed.CompletedFrames, "")
}
func (r *liveRegistry) settleWrite(e *liveEntry, record *liveWriteEntry, phase string, attempted bool, frames int, issue FailureCode) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if record.status.Phase != "pending" {
		return
	}
	record.status.Phase = phase
	record.status.Attempted = attempted
	record.status.CompletedFrames = frames
	record.status.Failure = issue
	if e.activeWrite == record.status.OperationID {
		e.activeWrite = ""
	}
}
func (s *Service) LiveWriteStatus(ctx context.Context, p trust.Principal, id, operation string) (LiveWriteStatus, error) {
	if !trust.ValidID(operation) {
		return LiveWriteStatus{}, Failure{Code: Validation}
	}
	e, err := s.live.lookup(p, id)
	if err != nil {
		return LiveWriteStatus{}, err
	}
	if err = s.live.source(ctx, e, false); err != nil {
		return LiveWriteStatus{}, err
	}
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.retired {
		return LiveWriteStatus{}, Failure{Code: NotFound}
	}
	if record := e.writes[operation]; record != nil {
		return record.status, nil
	}
	if e.closeWrite != nil && e.closeWrite.status.OperationID == operation {
		return e.closeWrite.status, nil
	}
	return LiveWriteStatus{}, Failure{Code: NotFound}
}

// CloseLiveCall attempts one provider close frame. Its receipt is write evidence
// only. Immediate native/local fencing uses Release, not this network operation.
func (s *Service) CloseLiveCall(ctx context.Context, p trust.Principal, id, operation string) (LiveWriteStatus, error) {
	if !trust.ValidID(operation) {
		return LiveWriteStatus{}, Failure{Code: Validation}
	}
	r := s.live
	e, err := r.lookup(p, id)
	if err != nil {
		return LiveWriteStatus{}, err
	}
	if err = r.source(ctx, e, false); err != nil {
		return LiveWriteStatus{}, err
	}
	if err = ctx.Err(); err != nil {
		return LiveWriteStatus{}, err
	}
	r.mu.Lock()
	e.mu.Lock()
	if r.closed || e.retired {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveWriteStatus{}, Failure{Code: NotFound}
	}
	if e.closeWrite != nil {
		if e.closeWrite.status.OperationID != operation {
			e.mu.Unlock()
			r.mu.Unlock()
			return LiveWriteStatus{}, Failure{Code: RequestRejected}
		}
		out := e.closeWrite.status
		e.mu.Unlock()
		r.mu.Unlock()
		return out, nil
	}
	if e.writes[operation] != nil {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveWriteStatus{}, Failure{Code: RequestRejected}
	}
	if e.phase != "ready" || e.transport == nil || e.controlCtx.Err() != nil {
		e.mu.Unlock()
		r.mu.Unlock()
		return LiveWriteStatus{}, Failure{Code: ModelUnavailable}
	}
	record := &liveWriteEntry{status: LiveWriteStatus{OperationID: operation, Phase: "pending"}}
	e.closeWrite = record
	e.phase = "closing"
	e.workers++
	r.wg.Add(1)
	out := record.status
	e.mu.Unlock()
	r.mu.Unlock()
	go r.sendClose(e, record)
	return out, nil
}
func (r *liveRegistry) sendClose(e *liveEntry, record *liveWriteEntry) {
	defer r.workerDone(e)
	ctx, cancel := context.WithTimeout(e.controlCtx, liveWriteTimeout)
	defer cancel()
	if err := r.source(ctx, e, false); err != nil {
		r.settleWrite(e, record, "not_sent", false, 0, liveCause(err))
		r.fail(e, liveCause(err))
		return
	}
	e.mu.Lock()
	allowed := !e.retired && ctx.Err() == nil
	if allowed {
		record.status.Attempted = true
	}
	e.mu.Unlock()
	if !allowed {
		r.settleWrite(e, record, "not_sent", false, 0, ModelUnavailable)
		return
	}
	complete, err := e.transport.RequestClose(ctx)
	if err != nil || !complete {
		issue := liveCause(err)
		if err == nil {
			issue = InvalidOutput
		}
		r.settleWrite(e, record, "uncertain", true, 0, issue)
		r.fail(e, issue)
		return
	}
	r.settleWrite(e, record, "written", true, 1, "")
	// Keep queued received events and read-only write evidence until Release/expiry.
	e.mu.Lock()
	if !e.retired {
		e.phase = "closed_local"
	}
	e.mu.Unlock()
	e.transport.Fence()
}
