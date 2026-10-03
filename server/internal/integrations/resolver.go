package integrations

import (
	"context"
	"errors"
	"floe/server/internal/authority"
	"floe/server/internal/trust"
	"floe/server/internal/views"
	"reflect"
	"sort"
	"strings"
)

func sourceResources(r Record) []string {
	for _, field := range []string{"calendar_ids", "entities"} {
		if raw, ok := r.Scope[field]; ok {
			values, _ := ConnectorScopeStrings(raw)
			return append([]string(nil), values...)
		}
	}
	keys := make([]string, 0, len(r.Scope))
	for key := range r.Scope {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	out := []string{}
	for _, key := range keys {
		if value, ok := r.Scope[key].(string); ok {
			out = append(out, key+":"+value)
		}
	}
	return out
}
func (s *Service) sourceSnapshotLocked(r Record, id views.ID, owner string) (views.SourceSnapshot, views.Reader, error) {
	runtime, ok := s.runtimes[r.ConnectionID]
	if !ok || runtime.Identity == nil || !runtime.IdentitySupported || r.IdentityUnverified || r.ProviderIdentity == "" {
		return views.SourceSnapshot{}, nil, errors.New("source identity unavailable")
	}
    registered,ok:=runtime.Readers[id]
    if !ok || registered.Reader==nil{return views.SourceSnapshot{},nil,errors.New("view unavailable")}
    reader,descriptor:=registered.Reader,registered.Descriptor
	if descriptor.SchemaVersion != 1 || descriptor.MaxItems < 1 || descriptor.MaxItems > 128 || descriptor.MaxBytes < 1 || descriptor.MaxBytes > 1<<20 {
		return views.SourceSnapshot{}, nil, errors.New("view descriptor unavailable")
	}
	device := ""
	if r.Device != nil {
		device = r.Device.DeviceID
	}
	return views.SourceSnapshot{SourceReference: views.SourceReference{ConnectorID: r.ConnectorID, ConnectionID: r.ConnectionID, ExecutionOwner: owner, Incarnation: r.Incarnation, Epoch: r.Epoch}, ConnectionRevision: r.Revision, PersonID: r.PersonID, DeviceID: device, ProviderIdentity: r.ProviderIdentity, IdentityGeneration: 1, Resources: sourceResources(r), Active: true, Descriptor: descriptor}, reader, nil
}
func (s *Service) ResolveSource(ctx context.Context, p trust.Principal, target views.SourceTarget) (out authority.ResolvedSource, err error) {
	if err = ctx.Err(); err != nil {
		return out, err
	}
	if target.ResourceID != string(target.ViewID)+":"+target.ConnectionID {
		return out, errors.New("invalid source target")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return out, err
	}
	err = s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.RLock()
		defer s.mu.RUnlock()
		if err := s.readyLocked(p.PersonID()); err != nil {
			return err
		}
		r, ok := s.state.Connections[target.ConnectionID]
		if !ok || r.PersonID != p.PersonID() || r.ConnectorID != target.ConnectorID || r.Device != nil && r.Device.DeviceID != p.DeviceID() || target.ConnectionRevision != 0 && target.ConnectionRevision != r.Revision {
			return errors.New("source changed")
		}
		snapshot, reader, err := s.sourceSnapshotLocked(r, target.ViewID, metadata.ExecutionOwner)
		if err != nil {
			return err
		}
		out = authority.ResolvedSource{Snapshot: snapshot, Reader: reader, Limits: views.Bounds{MaxItems: uint32(snapshot.Descriptor.MaxItems), MaxBytes: uint32(snapshot.Descriptor.MaxBytes)}}
		return nil
	})
	return out, err
}
func (s *Service) PreflightSource(ctx context.Context, p trust.Principal, expected views.SourceSnapshot) error {
	if err := s.check(p); err != nil {
		return err
	}
    metadata,err:=s.trust.ProducerMetadata()
    if err!=nil{return err}
    s.mu.RLock()
    r,ok:=s.state.Connections[expected.ConnectionID]
    current,_,snapshotErr:=s.sourceSnapshotLocked(r,views.ID(expected.Descriptor.ID),metadata.ExecutionOwner)
    s.mu.RUnlock()
    if !ok || snapshotErr!=nil || !reflect.DeepEqual(current,expected){return errors.New("source changed")}
	if err := s.preflight(ctx, r); err != nil {
		return err
	}
	return s.WithCurrentSource(p, expected, func(views.SourceSnapshot) error { return nil })
}
func (s *Service) WithCurrentSource(p trust.Principal, expected views.SourceSnapshot, consume func(views.SourceSnapshot) error) error {
	if consume == nil {
		return errors.New("source callback required")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return err
	}
	return s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.RLock()
		defer s.mu.RUnlock()
		if err := s.readyLocked(p.PersonID()); err != nil {
			return err
		}
		r, ok := s.state.Connections[expected.ConnectionID]
		if !ok || r.PersonID != p.PersonID() || r.Device != nil && r.Device.DeviceID != p.DeviceID() {
			return errors.New("source changed")
		}
		current, _, err := s.sourceSnapshotLocked(r, views.ID(expected.Descriptor.ID), metadata.ExecutionOwner)
		if err != nil || !reflect.DeepEqual(current, expected) {
			return errors.New("source changed")
		}
		namespace, subject, ok := strings.Cut(current.ProviderIdentity, ":")
		if !ok {
			return errors.New("source identity unavailable")
		}
		return s.runtimes[r.ConnectionID].Identity.WithVerified(binding(r), ProviderIdentity{namespace, subject, true, current.IdentityGeneration}, func() error { return consume(current) })
	})
}
