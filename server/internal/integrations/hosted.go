package integrations

import (
	"context"
	"errors"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"strings"
	"time"
)

// SetupPresentation is visible only to an admitted Go operator session.
// Product callers receive only the safe operation receipt from Start/Poll.
type SetupScopeField struct{ Name, Value string }
type SetupPresentation struct {
	ScopeFields                                                              []SetupScopeField
	OperationID, ConnectorName, State, AuthorizationURL, UserCode, ErrorCode string
	Revision                                                                 uint64
	SecretRequired                                                           bool
}

func (s *Service) hostedAttempt(operator trust.OperatorPrincipal, id string) (out attemptRecord, err error) {
	s.mu.RLock()
	out, ok := s.state.Attempts[id]
	s.mu.RUnlock()
	if !ok {
		return out, operation.Fail(operation.Missing, "operation_not_found")
	}
	err = s.trust.WithPairingOperation(operator, out.ClientID, out.PersonID, out.DeviceID, func(trust.PrincipalSnapshot) error {
		s.mu.RLock()
		defer s.mu.RUnlock()
		current, ok := s.state.Attempts[id]
		if s.unavailable || !ok || current.Record.ConnectionID != out.Record.ConnectionID {
			return operation.Fail(operation.Conflict, "operation_changed")
		}
		out = current
		return nil
	})
	return out, err
}
func (s *Service) hostedCommit(operator trust.OperatorPrincipal, expected attemptRecord, mutate func(*diskState, attemptRecord) error) error {
	return s.trust.WithPairingOperation(operator, expected.ClientID, expected.PersonID, expected.DeviceID, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if s.unavailable {
			return operation.Fail(operation.Unavailable, "integrations_unavailable")
		}
		a, ok := s.state.Attempts[expected.ID]
		if !ok || a.Record.ConnectionID != expected.Record.ConnectionID || a.Revision != expected.Revision || a.ClientID != expected.ClientID {
			return operation.Fail(operation.Conflict, "operation_changed")
		}
		next := clone(s.state)
		if err := mutate(&next, a); err != nil {
			return err
		}
		return s.persist(next)
	})
}
func (s *Service) HostedSetup(ctx context.Context, operator trust.OperatorPrincipal, id string) (SetupPresentation, error) {
	a, err := s.hostedAttempt(operator, id)
	if err != nil {
		return SetupPresentation{}, err
	}
	if a.Status == Pending {
		unlock := s.lock("setup/" + id)
		defer unlock()
		a, err = s.hostedAttempt(operator, id)
		if err != nil {
			return SetupPresentation{}, err
		}
		if a.Status == Pending {
			s.mu.RLock()
			runtime := s.runtimes[a.Record.ConnectionID]
			s.mu.RUnlock()
			if runtime.Setup == nil {
				return SetupPresentation{}, operation.Fail(operation.Unavailable, "setup_recovery_required")
			}
			bounded, cancel := context.WithTimeout(ctx, 20*time.Second)
			defer cancel()
			progress, e := runtime.Setup.Poll(bounded, AttemptRef{a.ID, a.Record.ConnectionID, 1})
			if e != nil {
				return presentation(a), nil
			}
			a, err = s.finishHosted(bounded, operator, a, runtime, progress)
			if err != nil {
				return SetupPresentation{}, err
			}
		}
	}
	return presentation(a), nil
}
func (s *Service) BeginHostedSetup(ctx context.Context, operator trust.OperatorPrincipal, id, secret string, expectedRevision uint64, scope map[string]any) (SetupPresentation, error) {
	unlock := s.lock("setup/" + id)
	defer unlock()
	a, err := s.hostedAttempt(operator, id)
	if err != nil {
		return SetupPresentation{}, err
	}
	if a.Status != AwaitingUser {
		if a.Started && expectedRevision+1 <= a.Revision {
			return presentation(a), nil
		}
		return SetupPresentation{}, operation.Fail(operation.Conflict, "operation_changed")
	}
	if a.Revision != expectedRevision {
		return SetupPresentation{}, operation.Fail(operation.Conflict, "operation_changed")
	}
	d, ok := DefinitionFor(a.ConnectorID)
	if !ok || InvalidConnectorToken(secret) || d.AuthKind == "secret" && secret == "" || d.AuthKind != "secret" && secret != "" {
		return SetupPresentation{}, operation.Fail(operation.Invalid, "validation")
	}
	selectedScope, scopeErr := ValidatedConnectorScope(d, scope)
	if scopeErr != nil {
		return SetupPresentation{}, operation.Fail(operation.Invalid, "invalid_scope")
	}
	if time.Since(time.UnixMilli(a.CreatedAt)) > 10*time.Minute {
		return SetupPresentation{}, operation.Fail(operation.Conflict, "setup_expired")
	}
	err = s.hostedCommit(operator, a, func(st *diskState, current attemptRecord) error {
		if current.Status != AwaitingUser {
			return operation.Fail(operation.Conflict, "operation_changed")
		}
		current.Record.Scope = selectedScope
		current.Started = true
		current.Status = Pending
		current.Revision++
		st.Attempts[id] = current
		a = current
		return nil
	})
	if err != nil {
		return SetupPresentation{}, err
	}
	// The exact owned operation is durable before writing a credential or opening OAuth.
	if secret != "" {
		if err = s.vault.Put(a.Record.Credential, secret); err != nil {
			return s.hostedFailure(ctx, operator, a, "credential_store_unavailable")
		}
	}
	factory := s.factories[a.ConnectorID]
	if factory == nil {
		return s.hostedFailure(ctx, operator, a, "connector_unavailable")
	}
	runtime, err := factory.Open(ctx, RuntimeConfig{cloneRecord(a.Record), binding(a.Record)})
	if err != nil {
		return s.hostedFailure(ctx, operator, a, "connector_unavailable")
	}
	s.mu.Lock()
	live, exists := s.state.Attempts[id]
	if !exists || live.Revision != a.Revision || live.Status != Pending {
		s.mu.Unlock()
		if runtime.Close != nil {
			runtime.Close()
		}
		return SetupPresentation{}, operation.Fail(operation.Conflict, "operation_changed")
	}
	s.runtimes[a.Record.ConnectionID] = runtime
	s.mu.Unlock()
	if runtime.Setup == nil {
		return s.hostedFailure(ctx, operator, a, "connector_unavailable")
	}
	bounded, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	progress, err := runtime.Setup.Begin(bounded, binding(a.Record))
	if err != nil {
		return s.hostedFailure(ctx, operator, a, "authorization_unavailable")
	}
	a, err = s.finishHosted(bounded, operator, a, runtime, progress)
	if err != nil {
		return SetupPresentation{}, err
	}
	return presentation(a), nil
}
func (s *Service) finishHosted(ctx context.Context, operator trust.OperatorPrincipal, a attemptRecord, runtime Runtime, progress AuthorizationProgress) (attemptRecord, error) {
	if progress.State != Pending && progress.State != Connected {
		_, err := s.hostedFailure(ctx, operator, a, "authorization_interrupted")
		if err != nil {
			return a, err
		}
		return s.hostedAttempt(operator, a.ID)
	}
	updated := a
	updated.Status = progress.State
	updated.AuthorizationURL = progress.AuthorizationURL
	updated.UserCode = progress.UserCode
	updated.ErrorCode = progress.ErrorCode
	if updated.Status == Connected {
		if runtime.IdentitySupported {
			if runtime.Identity == nil {
				return a, errors.New("provider identity unavailable")
			}
			identity, err := runtime.Identity.Preflight(ctx, binding(a.Record))
			if err != nil || !identity.Verified || identity.Subject == "" || identity.Generation != 1 {
				_, failure := s.hostedFailure(ctx, operator, a, "provider_identity_unavailable")
				if failure != nil {
					return a, failure
				}
				return s.hostedAttempt(operator, a.ID)
			}
			updated.Record.ProviderIdentity = identity.Namespace + ":" + identity.Subject
			updated.Record.IdentityUnverified = false
		}
		updated.AuthorizationURL = ""
		updated.UserCode = ""
	}
	if updated.Status == a.Status && updated.AuthorizationURL == a.AuthorizationURL && updated.UserCode == a.UserCode && updated.ErrorCode == a.ErrorCode {
		return a, nil
	}
	updated.Revision++
	err := s.hostedCommit(operator, a, func(st *diskState, current attemptRecord) error {
		if current.Status != Pending || !current.Started {
			return operation.Fail(operation.Conflict, "operation_changed")
		}
		if updated.Status == Connected {
			for _, existing := range st.Connections {
				if existing.PersonID == updated.PersonID && existing.ConnectorID == updated.ConnectorID {
					return operation.Fail(operation.Conflict, "connection_changed")
				}
			}
			st.Connections[updated.Record.ConnectionID] = cloneRecord(updated.Record)
		}
		st.Attempts[a.ID] = updated
		return nil
	})
	if err != nil {
		_ = s.ResumeCleanup(ctx)
		return a, err
	}
	return updated, nil
}
func (s *Service) hostedFailure(ctx context.Context, operator trust.OperatorPrincipal, a attemptRecord, code string) (SetupPresentation, error) {
	err := s.hostedCommit(operator, a, func(st *diskState, current attemptRecord) error {
		current.Status = Failed
		current.Revision++
		current.ErrorCode = code
		current.AuthorizationURL = ""
		current.UserCode = ""
		st.Attempts[a.ID] = current
		if current.Started {
			queueCleanup(st, []Record{current.Record}, nil)
		}
		a = current
		return nil
	})
	if err != nil {
		return SetupPresentation{}, err
	}
	_ = s.ResumeCleanup(ctx)
	return presentation(a), nil
}
func presentation(a attemptRecord) SetupPresentation {
	name := a.ConnectorID
	secret := false
	fields := []SetupScopeField{}
	if d, ok := DefinitionFor(a.ConnectorID); ok {
		name = d.Name
		secret = d.AuthKind == "secret"
		for _, key := range d.ScopeFields {
			value := ""
			switch v := a.Record.Scope[key].(type) {
			case string:
				value = v
			case []string:
				value = strings.Join(v, ", ")
			case []any:
				items, _ := ConnectorScopeStrings(v)
				value = strings.Join(items, ", ")
			}
			fields = append(fields, SetupScopeField{Name: key, Value: value})
		}
	}
	return SetupPresentation{ScopeFields: fields, OperationID: a.ID, ConnectorName: name, State: string(a.Status), AuthorizationURL: a.AuthorizationURL, UserCode: a.UserCode, ErrorCode: a.ErrorCode, Revision: a.Revision, SecretRequired: secret}
}
