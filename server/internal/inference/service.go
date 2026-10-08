package inference

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"floe/server/internal/trust"
	"sync"
	"time"
)

type Service struct {
	mu          sync.RWMutex
	unavailable bool
	trust       Trust
	secret      [32]byte
	config      InferenceConfig
	generation  uint64
	accounts    map[string]ModelAccount
	executor    ModelExecutor
	active      chan struct{}
	audit       *auditLog
}

func NewService(t Trust) (*Service, error) {
	if t == nil {
		return nil, errors.New("trust required")
	}
	s := &Service{trust: t, config: InferenceConfig{Routes: map[Purpose]PurposeRoute{}}, accounts: map[string]ModelAccount{}, generation: 1, active: make(chan struct{}, 4), audit: newAuditLog(256)}
	if _, err := rand.Read(s.secret[:]); err != nil {
		return nil, err
	}
	return s, nil
}
func (s *Service) Configure(config InferenceConfig, accounts map[string]ModelAccount, executor ModelExecutor) error {
	if ValidateConfig(config) != nil || executor == nil {
		return errors.New("invalid inference configuration")
	}
	routes := map[Purpose]PurposeRoute{}
	for p, r := range config.Routes {
		if accounts[r.TargetID] == nil || !ValidCapabilities(accounts[r.TargetID].Capabilities()) {
			return errors.New("configured target missing")
		}
		routes[p] = r
	}
	copied := map[string]ModelAccount{}
	for k, v := range accounts {
		copied[k] = v
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.unavailable {
		return errors.New("configuration uncertain")
	}
	s.config = InferenceConfig{Routes: routes}
	s.accounts = copied
	s.executor = executor
	s.generation++
	return nil
}
func (s *Service) current(ctx context.Context, p Purpose) (ResolvedModelTarget, string, ModelExecutor, error) {
	s.mu.RLock()
	if s.unavailable {
		s.mu.RUnlock()
		return ResolvedModelTarget{}, "", nil, Failure{Code: ModelUnavailable}
	}
	r, exists := s.config.Routes[p]
	generation := s.generation
	account := s.accounts[r.TargetID]
	executor := s.executor
	s.mu.RUnlock()
	if !exists {
		return ResolvedModelTarget{}, "", nil, Failure{Code: PurposeNotConfigured}
	}
	if !r.Enabled {
		return ResolvedModelTarget{}, "", nil, Failure{Code: PurposeDisabled}
	}
	if account == nil || executor == nil {
		return ResolvedModelTarget{}, "", nil, Failure{Code: ModelUnavailable}
	}
	if err := account.Ready(ctx); err != nil {
		return ResolvedModelTarget{}, "", nil, normalizeFailure(err)
	}
	identity := account.ReplayIdentity()
	if identity == "" {
		return ResolvedModelTarget{}, "", nil, Failure{Code: ProviderCredentialsUnavailable}
	}
	capabilities := account.Capabilities()
	if !ValidCapabilities(capabilities) {
		return ResolvedModelTarget{}, "", nil, Failure{Code: ModelUnavailable}
	}
	modelIdentity := account.ModelIdentity()
	if !validModelIdentity(modelIdentity) {
		return ResolvedModelTarget{}, "", nil, Failure{Code: ModelUnavailable}
	}
	budgetProfile := ResolveModelBudgetProfile(account.BudgetOverride())
	if budgetProfile.Validate() != nil {
		return ResolvedModelTarget{}, "", nil, Failure{Code: ModelUnavailable}
	}
	target := ResolvedModelTarget{targetID: r.TargetID, effort: r.ReasoningEffort, accountIdentity: identity, generation: generation, capabilities: append([]string(nil), capabilities...), modelIdentity: modelIdentity, budgetProfile: budgetProfile}
	material, _ := json.Marshal(struct {
		Purpose                    Purpose
		TargetID, Effort, Identity string
		Generation                 uint64
		Capabilities               []string
		BudgetProfile              any
	}{p, r.TargetID, r.ReasoningEffort, identity, generation, capabilities, budgetProfile.effectiveRevisionMaterial()})
	mac := hmac.New(sha256.New, s.secret[:])
	mac.Write(material)
	revision := hex.EncodeToString(mac.Sum(nil))
	s.mu.RLock()
	unchanged := s.generation == generation && s.config.Routes[p] == r
	s.mu.RUnlock()
	if !unchanged {
		return ResolvedModelTarget{}, "", nil, Failure{Code: CapabilityChanged}
	}
	return target, revision, executor, nil
}
func (s *Service) Snapshot(ctx context.Context) (PurposeInventory, error) {
	var out PurposeInventory
	for _, p := range Purposes {
		target, revision, _, err := s.current(ctx, p)
		if err != nil {
			var f Failure
			if errors.As(err, &f) {
				if f.Code == PurposeNotConfigured {
					out.set(p, PurposeCapability{Status: NotConfigured})
					continue
				}
				if f.Code == PurposeDisabled {
					out.set(p, PurposeCapability{Status: Disabled})
					continue
				}
			}
			return PurposeInventory{}, err
		}
		out.set(p, PurposeCapability{
			Status: Available, CapabilityRevision: revision, Capabilities: append([]string(nil), target.capabilities...),
			BudgetProfile: target.budgetProfile, ModelIdentity: target.modelIdentity,
		})
	}
	return out, nil
}
func (s *Service) ObservePurposes(ctx context.Context, p trust.Principal) (PurposeInventory, error) {
	if _, err := s.trust.ActiveIssuer(p); err != nil {
		return PurposeInventory{}, Failure{Code: Unauthorized}
	}
	inventory, err := s.Snapshot(ctx)
	if err != nil {
		return PurposeInventory{}, err
	}
	if err = s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error { return nil }); err != nil {
		return PurposeInventory{}, Failure{Code: Unauthorized}
	}
	return inventory, nil
}
func (s *Service) InvokeAgent(ctx context.Context, p trust.Principal, in AgentInvocation) (out AgentResult, err error) {
	if err = ValidateAgentInvocation(in); err != nil {
		return out, err
	}
	if _, err = s.trust.ActiveIssuer(p); err != nil {
		return out, Failure{Code: Unauthorized}
	}
	target, revision, executor, err := s.current(ctx, in.Purpose)
	if err != nil {
		return out, err
	}
	if revision != in.CapabilityRevision {
		return out, Failure{Code: CapabilityChanged}
	}
	if err = ValidateModelBudgetInput(target.budgetProfile, in); err != nil {
		return out, err
	}
	if !SupportsAgent(target.capabilities, in) {
		return out, Failure{Code: RequestRejected}
	}
	select {
	case s.active <- struct{}{}:
		defer func() { <-s.active }()
	default:
		return out, Failure{Code: ModelBusy}
	}
	if err = ctx.Err(); err != nil {
		return out, err
	}
	// The captured trust stamp and private account/config digest are checked immediately before handoff.
	if err = s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error { return nil }); err != nil {
		return out, Failure{Code: Unauthorized}
	}
	if _, fresh, _, e := s.current(ctx, in.Purpose); e != nil || fresh != revision {
		return out, Failure{Code: CapabilityChanged}
	}
	trace := newTraceID()
	start := time.Now()
	ctx, cancel := context.WithTimeout(ctx, 40*time.Second)
	defer cancel()
	out, err = executor.InvokeAgent(ctx, target, in)
	usage := out.Usage
	if err != nil {
		f := normalizeFailure(err)
		f.TraceID = trace
		f.Dispatched = true
		f.AttemptID, f.Purpose, f.CapabilityRevision = in.AttemptID, in.Purpose, revision
		if ValidateUsage(usage) == nil {
			f.Usage = usage
		}
		s.audit.add(newAuditRecord(trace, string(in.Purpose), in.DataClasses, in, target.accountIdentity, string(f.Code), nil, usage, start))
		return AgentResult{}, f
	}
	if e := ValidateUsage(usage); e != nil {
		err = e
	} else if e = s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error { return nil }); e != nil {
		err = Failure{Code: PermissionDenied}
	} else if _, fresh, _, e := s.current(ctx, in.Purpose); e != nil || fresh != revision {
		err = Failure{Code: CapabilityChanged}
	} else {
		err = ValidateAgentResult(in, out)
	}
	if err != nil {
		f := normalizeFailure(err)
		f.TraceID = trace
		f.Dispatched = true
		f.AttemptID, f.Purpose, f.CapabilityRevision = in.AttemptID, in.Purpose, revision
		f.Usage = usage
		s.audit.add(newAuditRecord(trace, string(in.Purpose), in.DataClasses, in, target.accountIdentity, string(f.Code), nil, usage, start))
		return AgentResult{}, f
	}
	out.Purpose = in.Purpose
	out.CapabilityRevision = revision
	out.AttemptID = in.AttemptID
	out.TraceID = trace
	s.audit.add(newAuditRecord(trace, string(in.Purpose), in.DataClasses, in, target.accountIdentity, "completed", out.Output, usage, start))
	return out, nil
}
func (s *Service) InvokeStructured(ctx context.Context, p trust.OperatorPrincipal, in StructuredInvocation) (out StructuredResult, err error) {
	if err = ValidateStructuredInvocation(in); err != nil {
		return out, err
	}
	if err = s.trust.WithCurrentOperator(p, func() error { return nil }); err != nil {
		return out, Failure{Code: Unauthorized}
	}
	target, revision, executor, err := s.current(ctx, in.Purpose)
	if err != nil {
		return out, err
	}
	if revision != in.CapabilityRevision {
		return out, Failure{Code: CapabilityChanged}
	}
	if err = ValidateStructuredModelBudgetInput(target.budgetProfile, in); err != nil {
		return out, err
	}
	select {
	case s.active <- struct{}{}:
		defer func() { <-s.active }()
	default:
		return out, Failure{Code: ModelBusy}
	}
	if err = ctx.Err(); err != nil {
		return out, err
	}
	if err = s.trust.WithCurrentOperator(p, func() error { return nil }); err != nil {
		return out, Failure{Code: Unauthorized}
	}
	if _, fresh, _, e := s.current(ctx, in.Purpose); e != nil || fresh != revision {
		return out, Failure{Code: CapabilityChanged}
	}
	trace := newTraceID()
	start := time.Now()
	ctx, cancel := context.WithTimeout(ctx, 40*time.Second)
	defer cancel()
	out, err = executor.InvokeStructured(ctx, target, in)
	usage := out.Usage
	if err == nil {
		if e := ValidateUsage(usage); e != nil {
			err = e
		} else if e = s.trust.WithCurrentOperator(p, func() error { return nil }); e != nil {
			err = Failure{Code: PermissionDenied}
		} else if _, fresh, _, e := s.current(ctx, in.Purpose); e != nil || fresh != revision {
			err = Failure{Code: CapabilityChanged}
		} else {
			err = ValidateStructuredOutput(in, out.Output)
		}
	}
	if err != nil {
		f := normalizeFailure(err)
		f.TraceID = trace
		f.Dispatched = true
		f.AttemptID, f.Purpose, f.CapabilityRevision = in.AttemptID, in.Purpose, revision
		f.Usage = usage
		s.audit.add(newAuditRecord(trace, string(in.Purpose), in.DataClasses, in, target.accountIdentity, string(f.Code), nil, usage, start))
		return StructuredResult{}, f
	}
	out.Purpose = in.Purpose
	out.CapabilityRevision = revision
	out.AttemptID = in.AttemptID
	out.TraceID = trace
	s.audit.add(newAuditRecord(trace, string(in.Purpose), in.DataClasses, in, target.accountIdentity, "completed", out.Output, usage, start))
	return out, nil
}
func normalizeFailure(err error) Failure {
	var f Failure
	if errors.As(err, &f) {
		return f
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return Failure{Code: ModelTimeout}
	}
	return Failure{Code: ModelUnavailable}
}
func newTraceID() string {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		panic("secure randomness unavailable")
	}
	return hex.EncodeToString(b)
}
func (s *Service) Traces(p trust.OperatorPrincipal, limit int) ([]AuditRecord, error) {
	if err := s.trust.WithCurrentOperator(p, func() error { return nil }); err != nil {
		return nil, err
	}
	if limit < 0 || limit > 20 {
		limit = 20
	}
	return s.audit.list(limit), nil
}
func (s *Service) Trace(p trust.OperatorPrincipal, id string) (AuditRecord, error) {
	if err := s.trust.WithCurrentOperator(p, func() error { return nil }); err != nil {
		return AuditRecord{}, Failure{Code: Unauthorized}
	}
	record, ok := s.audit.get(id)
	if !ok {
		return AuditRecord{}, Failure{Code: NotFound}
	}
	return record, nil
}

func (s *Service) DenyConfiguration() { s.mu.Lock(); defer s.mu.Unlock(); s.unavailable = true }
