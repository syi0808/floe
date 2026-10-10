package integrations

import (
	"context"
	"errors"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"reflect"
	"sort"
)

// List publishes owned, cached metadata and closes concurrent lifecycle changes
// before returning. Provider results never determine connection ownership.
func (s *Service) List(ctx context.Context, p trust.Principal) (ConnectionsResult, error) {
	if err := s.check(p); err != nil {
		return ConnectionsResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	metadata, err := s.trust.ProducerMetadata()
	if err != nil {
		return ConnectionsResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	s.mu.RLock()
	state := clone(s.state)
	runtimes := copyRuntimes(s.runtimes)
	s.mu.RUnlock()
	ids := []string{}
	for id, r := range state.Connections {
		if r.PersonID == p.PersonID() && (r.Device == nil || r.Device.DeviceID == p.DeviceID()) {
			ids = append(ids, id)
		}
	}
	sort.Strings(ids)
	snapshots := make([]Snapshot, 0, len(ids))
	for _, id := range ids {
		if err := ctx.Err(); err != nil {
			return ConnectionsResult{}, operation.Fail(operation.Unavailable, "cancelled")
		}
		r := state.Connections[id]
		runtime, ok := runtimes[id]
		if !ok || runtime.Snapshot == nil {
			return ConnectionsResult{}, operation.Fail(operation.Unavailable, "connections_unavailable")
		}
		snapshot, err := runtime.Snapshot.Snapshot(ctx)
		if err != nil || snapshot.Descriptor.ID != r.ConnectorID || snapshot.Connection.ConnectorID != r.ConnectorID {
			return ConnectionsResult{}, operation.Fail(operation.Unavailable, "connection_snapshot_invalid")
		}
		snapshot = cloneSnapshot(snapshot)
		snapshot.Connection.ConnectionID = r.ConnectionID
		snapshot.Connection.PersonID = r.PersonID
		snapshot.Connection.DeviceBinding = nil
		if r.Device != nil {
			device := *r.Device
			snapshot.Connection.DeviceBinding = &device
		}
		snapshot.Connection.Authority = &SourceAuthority{ExecutionOwner: metadata.ExecutionOwner, Incarnation: r.Incarnation, Epoch: r.Epoch, IdentityUnverified: r.IdentityUnverified}
		snapshots = append(snapshots, snapshot)
	}
	err = s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.RLock()
		defer s.mu.RUnlock()
		if err := s.readyLocked(p.PersonID()); err != nil {
			return err
		}
		if s.state.Revision != state.Revision {
			return operation.Fail(operation.Conflict, "connection_changed")
		}
		return ctx.Err()
	})
	if err != nil {
		return ConnectionsResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	return ConnectionsResult{SchemaVersion: 1, PersonID: p.PersonID(), DeviceID: p.DeviceID(), Connections: snapshots}, nil
}
func (s *Service) preflight(ctx context.Context, expected Record) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	s.mu.RLock()
	current, ok := s.state.Connections[expected.ConnectionID]
	runtime := s.runtimes[expected.ConnectionID]
	s.mu.RUnlock()
	if !ok || !reflect.DeepEqual(current, expected) || runtime.Identity == nil || !runtime.IdentitySupported {
		return errors.New("source identity unavailable")
	}
	id, err := runtime.Identity.Preflight(ctx, binding(expected))
	if err != nil || !id.Verified || id.Generation != binding(expected).Generation || id.Namespace+":"+id.Subject != expected.ProviderIdentity {
		return errors.New("source identity changed")
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	if !reflect.DeepEqual(s.state.Connections[expected.ConnectionID], expected) {
		return errors.New("source changed")
	}
	return nil
}
func cloneSnapshot(s Snapshot) Snapshot {
	s.Descriptor.Capabilities = append([]Capability(nil), s.Descriptor.Capabilities...)
	for i := range s.Descriptor.Capabilities {
		s.Descriptor.Capabilities[i].RequiredScopes = append([]string{}, s.Descriptor.Capabilities[i].RequiredScopes...)
	}
	s.Descriptor.Views = append(s.Descriptor.Views[:0:0], s.Descriptor.Views...)
	execution := map[string]any{}
	for k, v := range s.Descriptor.Execution {
		execution[k] = v
	}
	s.Descriptor.Execution = execution
	s.Connection.GrantedScopes = append([]string{}, s.Connection.GrantedScopes...)
	if s.Connection.LastSuccessAtUnixMS != nil {
		v := *s.Connection.LastSuccessAtUnixMS
		s.Connection.LastSuccessAtUnixMS = &v
	}
	if s.Connection.LastFailure != nil {
		v := *s.Connection.LastFailure
		s.Connection.LastFailure = &v
	}
	if s.Connection.DeviceBinding != nil {
		v := *s.Connection.DeviceBinding
		s.Connection.DeviceBinding = &v
	}
	if s.Connection.Authority != nil {
		v := *s.Connection.Authority
		s.Connection.Authority = &v
	}
	s.Views = append(s.Views[:0:0], s.Views...)
	return s
}
