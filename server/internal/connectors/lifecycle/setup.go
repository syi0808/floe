// Package lifecycle adapts provider setup to immutable connection bindings.
package lifecycle

import (
	"context"
	"errors"
	"floe/server/internal/connections"
	"floe/server/internal/credentials"
	"floe/server/internal/integrations"
	"net/url"
	"strings"
	"sync"
)

type OAuth interface {
	Action(context.Context, string) (any, error)
	Ready() bool
}
type IdentifiedOAuth interface {
	OAuth
	connections.ProviderIdentityRuntime
	connections.ProviderIdentityStatusRuntime
	connections.ProviderIdentityFenceRuntime
}
type oauthSetup struct {
	mu        sync.Mutex
	driver    OAuth
	binding   integrations.CredentialBinding
	cached    integrations.CredentialStatus
	cancelled bool
}

func NewOAuth(driver OAuth, binding integrations.CredentialBinding) integrations.Setup {
	return &oauthSetup{driver: driver, binding: binding, cached: integrations.CredentialStatus{Ready: driver.Ready()}}
}
func (s *oauthSetup) check(b integrations.CredentialBinding) error {
	if b != s.binding || s.cancelled {
		return errors.New("credential binding changed")
	}
	return nil
}
func (s *oauthSetup) Begin(ctx context.Context, b integrations.CredentialBinding) (integrations.AuthorizationProgress, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := s.check(b); err != nil {
		return integrations.AuthorizationProgress{}, err
	}
	return s.action(ctx, "login", integrations.AttemptRef{ConnectionID: b.ConnectionID, BindingGeneration: b.Generation})
}
func (s *oauthSetup) Poll(ctx context.Context, a integrations.AttemptRef) (integrations.AuthorizationProgress, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.cancelled || a.ConnectionID != s.binding.ConnectionID || a.BindingGeneration != s.binding.Generation {
		return integrations.AuthorizationProgress{}, errors.New("attempt changed")
	}
	return s.action(ctx, "status", a)
}
func (s *oauthSetup) action(ctx context.Context, action string, a integrations.AttemptRef) (integrations.AuthorizationProgress, error) {
	v, err := s.driver.Action(ctx, action)
	if err != nil {
		return integrations.AuthorizationProgress{}, err
	}
	status, link, code, ok := connections.OauthActionStatus(v)
	if !ok || len(link) > 4096 || len(code) > 128 || strings.ContainsAny(code, "\r\n\x00") {
		return integrations.AuthorizationProgress{}, errors.New("invalid setup response")
	}
	if link != "" {
		u, err := url.Parse(link)
		if err != nil || u.Scheme != "https" || u.Host == "" || u.User != nil {
			return integrations.AuthorizationProgress{}, errors.New("invalid authorization URL")
		}
	}
	s.cached.Ready = status == "connected"
	return integrations.AuthorizationProgress{Attempt: a, State: integrations.AuthorizationState(status), AuthorizationURL: link, UserCode: code}, nil
}
func (s *oauthSetup) Cancel(ctx context.Context, a integrations.AttemptRef) error {
	if a.ConnectionID != s.binding.ConnectionID || a.BindingGeneration != s.binding.Generation {
		return errors.New("attempt changed")
	}
	return s.Disconnect(ctx, s.binding)
}
func (s *oauthSetup) Disconnect(ctx context.Context, b integrations.CredentialBinding) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if b != s.binding {
		return errors.New("credential binding changed")
	}
	s.cancelled = true
	s.cached = integrations.CredentialStatus{}
	_, err := s.driver.Action(ctx, "logout")
	return err
}
func (s *oauthSetup) CachedStatus(b integrations.CredentialBinding) integrations.CredentialStatus {
	s.mu.Lock()
	defer s.mu.Unlock()
	if b != s.binding || s.cancelled {
		return integrations.CredentialStatus{ReasonCode: "credential_unavailable"}
	}
	return s.cached
}

type identity struct {
	driver    IdentifiedOAuth
	binding   integrations.CredentialBinding
	namespace string
}

func NewIdentity(driver IdentifiedOAuth, b integrations.CredentialBinding, namespace string) integrations.IdentityVerifier {
	return identity{driver, b, namespace}
}
func (i identity) Preflight(ctx context.Context, b integrations.CredentialBinding) (integrations.ProviderIdentity, error) {
	if b != i.binding {
		return integrations.ProviderIdentity{}, errors.New("credential binding changed")
	}
	subject, err := i.driver.ProviderIdentity(ctx)
	if err != nil || subject == "" {
		return integrations.ProviderIdentity{}, errors.New("provider identity unavailable")
	}
	return integrations.ProviderIdentity{Namespace: i.namespace, Subject: subject, Verified: true, Generation: b.Generation}, nil
}
func (i identity) WithVerified(b integrations.CredentialBinding, p integrations.ProviderIdentity, consume func() error) error {
	if b != i.binding || p.Namespace != i.namespace || !p.Verified || p.Generation != b.Generation {
		return errors.New("provider identity changed")
	}
	return i.driver.WithVerifiedProviderIdentity(b.Slot, p.Subject, consume)
}

type secretSetup struct {
	mu      sync.Mutex
	binding integrations.CredentialBinding
	ready   bool
}

func NewSecret(v credentials.Store, b integrations.CredentialBinding) integrations.Setup {
	value, err := v.Get(b.Slot)
	return &secretSetup{binding: b, ready: err == nil && value != ""}
}
func (s *secretSetup) Begin(ctx context.Context, b integrations.CredentialBinding) (integrations.AuthorizationProgress, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if b != s.binding || !s.ready {
		return integrations.AuthorizationProgress{}, errors.New("credential unavailable")
	}
	return integrations.AuthorizationProgress{State: integrations.Connected}, nil
}
func (s *secretSetup) Poll(ctx context.Context, a integrations.AttemptRef) (integrations.AuthorizationProgress, error) {
	return integrations.AuthorizationProgress{}, errors.New("unsupported")
}
func (s *secretSetup) Cancel(ctx context.Context, a integrations.AttemptRef) error {
	return errors.New("unsupported")
}
func (s *secretSetup) Disconnect(ctx context.Context, b integrations.CredentialBinding) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if b != s.binding {
		return errors.New("credential changed")
	}
	s.ready = false
	return nil
}
func (s *secretSetup) CachedStatus(b integrations.CredentialBinding) integrations.CredentialStatus {
	s.mu.Lock()
	defer s.mu.Unlock()
	return integrations.CredentialStatus{Ready: b == s.binding && s.ready}
}
