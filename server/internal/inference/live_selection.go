package inference

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"floe/server/internal/trust"
)

// Live is observed separately from the generation DTO: its capabilities and
// effective limits cannot masquerade as a chat/token-budget profile.
type LivePurposeCapability struct {
	Status             PurposeStatus
	CapabilityRevision string
	Profile            *LiveProfile
}
type selectedLive struct {
	account                      LiveModelAccount
	identity, targetID, revision string
	generation                   uint64
	profile                      LiveProfile
}

func validRouteAccount(purpose Purpose, account ModelAccount) bool {
	if purpose != LivePresentation {
		return validProtocolAccount(account)
	}
	live, ok := account.(LiveModelAccount)
	if !ok {
		return false
	}
	profile, ok := live.LiveProfile()
	return ok && profile.Validate() == nil && profile.ModelIdentity == account.ModelIdentity()
}
func (s *Service) currentLive(ctx context.Context) (selectedLive, error) {
	return s.selectLive(ctx, true)
}

// Existing live leases can check immutable routing/account facts without
// initiating another credential refresh. The transport owns expiry/rotation.
func (s *Service) selectLive(ctx context.Context, checkReadiness bool) (selectedLive, error) {
	if err := ctx.Err(); err != nil {
		return selectedLive{}, err
	}
	s.mu.RLock()
	unavailable := s.unavailable
	generation := s.generation
	route, configured := s.config.Routes[LivePresentation]
	account := s.accounts[route.TargetID]
	s.mu.RUnlock()
	if unavailable {
		return selectedLive{}, Failure{Code: ModelUnavailable}
	}
	if !configured {
		return selectedLive{}, Failure{Code: PurposeNotConfigured}
	}
	if !route.Enabled {
		return selectedLive{}, Failure{Code: PurposeDisabled}
	}
	if !ValidPurposeEffort(LivePresentation, route.ReasoningEffort) || !validRouteAccount(LivePresentation, account) {
		return selectedLive{}, Failure{Code: ModelUnavailable}
	}
	if checkReadiness {
		if err := account.Ready(ctx); err != nil {
			return selectedLive{}, err
		}
	}
	live := account.(LiveModelAccount)
	profile, ok := live.LiveProfile()
	identity := account.ReplayIdentity()
	if !ok || profile.Validate() != nil || profile.ModelIdentity != account.ModelIdentity() || identity == "" {
		return selectedLive{}, Failure{Code: ModelUnavailable}
	}
	profile = CloneLiveProfile(profile)
	material, err := json.Marshal(struct {
		Purpose            Purpose
		TargetID, Identity string
		Generation         uint64
		Profile            LiveProfile
	}{LivePresentation, route.TargetID, identity, generation, profile})
	if err != nil {
		return selectedLive{}, Failure{Code: ModelUnavailable}
	}
	mac := hmac.New(sha256.New, s.secret[:])
	mac.Write(material)
	revision := hex.EncodeToString(mac.Sum(nil))
	s.mu.RLock()
	current := !s.unavailable && s.generation == generation && s.config.Routes[LivePresentation] == route
	s.mu.RUnlock()
	if !current {
		return selectedLive{}, Failure{Code: CapabilityChanged}
	}
	if err := ctx.Err(); err != nil {
		return selectedLive{}, err
	}
	return selectedLive{account: live, identity: identity, targetID: route.TargetID, revision: revision, generation: generation, profile: profile}, nil
}
func (s *Service) ObserveLivePurpose(ctx context.Context, p trust.Principal) (LivePurposeCapability, error) {
	if _, err := s.trust.ActiveIssuer(p); err != nil {
		return LivePurposeCapability{}, Failure{Code: Unauthorized}
	}
	selected, err := s.currentLive(ctx)
	out := LivePurposeCapability{}
	if err != nil {
		if failure, ok := err.(Failure); ok && failure.Code == PurposeNotConfigured {
			out.Status = NotConfigured
		} else if ok && failure.Code == PurposeDisabled {
			out.Status = Disabled
		} else {
			return LivePurposeCapability{}, err
		}
	} else {
		profile := CloneLiveProfile(selected.profile)
		out = LivePurposeCapability{Status: Available, CapabilityRevision: selected.revision, Profile: &profile}
	}
	if err := s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error { return nil }); err != nil {
		return LivePurposeCapability{}, Failure{Code: Unauthorized}
	}
	return out, nil
}
