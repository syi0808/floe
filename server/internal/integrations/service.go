// Package integrations owns source lifecycle, resource CAS and durable cleanup.
package integrations

import (
	"context"
	"errors"
	"floe/server/internal/credentials"
	"floe/server/internal/operation"
	"floe/server/internal/storage"
	"floe/server/internal/trust"
	"reflect"
	"sort"
	"sync"
	"time"
)

type Service struct {
	mu          sync.RWMutex
	files       *storage.Files
	trust       Trust
	vault       credentials.Store
	factories   map[string]RuntimeFactory
	state       diskState
	runtimes    map[string]Runtime
	unavailable bool
	lifecycles  sync.Map
}

func New(ctx context.Context, files *storage.Files, t Trust, v credentials.Store, factories map[string]RuntimeFactory) (*Service, error) {
	if files == nil || t == nil || v == nil {
		return nil, errors.New("integration dependency unavailable")
	}
	state, err := readState(files)
	if err != nil {
		return nil, err
	}
	s := &Service{files: files, trust: t, vault: v, factories: map[string]RuntimeFactory{}, state: state, runtimes: map[string]Runtime{}}
	for k, f := range factories {
		s.factories[k] = f
	}
	// Interrupted authorization never resumes with a provider: journal cleanup before opening sources.
	for id, a := range s.state.Attempts {
		if a.Status == Pending {
			c := cleanupRecord{ID: trust.NewID(), Records: []Record{a.Record}, RuntimeDone: map[string]bool{}, VaultDone: map[string]bool{}}
			s.state.Cleanup[c.ID] = c
			a.Status = Failed
			a.Revision++
			a.ErrorCode = "authorization_interrupted"
			a.AuthorizationURL = ""
			a.UserCode = ""
			s.state.Attempts[id] = a
		}
	}
	if err = s.persist(s.state); err != nil {
		return nil, err
	}
	_ = s.ResumeCleanup(ctx)
	for _, r := range s.state.Connections {
		if f := s.factories[r.ConnectorID]; f != nil {
			runtime, e := f.Open(ctx, RuntimeConfig{cloneRecord(r), binding(r)})
			if e == nil {
				s.runtimes[r.ConnectionID] = runtime
			}
		}
	}
	return s, nil
}
func (s *Service) Close() {
	s.mu.Lock()
	runtimes := s.runtimes
	s.runtimes = map[string]Runtime{}
	s.mu.Unlock()
	for _, r := range runtimes {
		if r.Close != nil {
			r.Close()
		}
	}
}
func (s *Service) lock(key string) func() {
	v, _ := s.lifecycles.LoadOrStore(key, &sync.Mutex{})
	m := v.(*sync.Mutex)
	m.Lock()
	return m.Unlock
}
func (s *Service) check(p trust.Principal) error {
	return s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.RLock()
		defer s.mu.RUnlock()
		return s.readyLocked(p.PersonID())
	})
}
func (s *Service) readyLocked(person string) error {
	if s.unavailable || s.files.Available() != nil {
		return operation.Fail(operation.Unavailable, "integrations_unavailable")
	}
	for _, c := range s.state.Cleanup {
		for _, r := range c.Records {
			if r.PersonID == person {
				return operation.Fail(operation.Conflict, "connection_cleanup_pending")
			}
		}
	}
	return nil
}
func (s *Service) commit(p trust.Principal, mutate func(*diskState) error) error {
	return s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if err := s.readyLocked(p.PersonID()); err != nil {
			return err
		}
		next := clone(s.state)
		if err := mutate(&next); err != nil {
			return err
		}
		if reflect.DeepEqual(next, s.state) {
			return nil
		}
		return s.persist(next)
	})
}
func (s *Service) Catalog(ctx context.Context, p trust.Principal) operation.Result {
	if err := s.check(p); err != nil {
		return trust.Result(err)
	}
	s.mu.RLock()
	state := clone(s.state)
	runtimes := copyRuntimes(s.runtimes)
	s.mu.RUnlock()
	items := []any{}
	meta, err := s.trust.ProducerMetadata()
	if err != nil {
		return trust.Result(err)
	}
	for _, d := range Definitions() {
		item := map[string]any{"id": d.ID, "name": d.Name, "auth_kind": d.AuthKind, "available": s.factories[d.ID] != nil, "status": "disconnected", "required_scopes": d.RequiredScopes, "scope_fields": d.ScopeFields, "capabilities": ConnectorCapabilities(d)}
		if s.factories[d.ID] == nil {
			item["status"] = "unavailable"
		}
		for _, r := range state.Connections {
			if r.PersonID == p.PersonID() && r.ConnectorID == d.ID {
				status := "error"
				if runtime, ok := runtimes[r.ConnectionID]; ok && runtime.Setup != nil && runtime.Setup.CachedStatus(binding(r)).Ready {
					status = "connected"
				}
				item["status"] = status
				item["connection_id"] = r.ConnectionID
				item["connection_revision"] = r.Revision
				item["incarnation"] = r.Incarnation
				item["epoch"] = r.Epoch
				item["execution_owner"] = meta.ExecutionOwner
				item["identity_unverified"] = r.IdentityUnverified
				item["scope"] = r.Scope
			}
		}
		for _, a := range state.Attempts {
			if a.ClientID == p.ClientID() && a.ConnectorID == d.ID && a.Status == Pending {
				item["status"] = "connecting"
			}
		}
		items = append(items, item)
	}
	if err = s.check(p); err != nil {
		return trust.Result(err)
	}
	return operation.Accept(map[string]any{"schema_version": 1, "person_id": p.PersonID(), "device_id": p.DeviceID(), "connectors": items, "revision": state.Revision})
}
func (s *Service) Start(ctx context.Context, p trust.Principal, id string, in ConnectRequest) operation.Result {
	d, ok := DefinitionFor(id)
	if !ok {
		return operation.Reject(operation.Missing, "connector_not_found")
	}
	if in.SchemaVersion != 1 || !trust.ValidID(in.OperationID) || in.Scope == nil || in.ExpectedCatalogRevision == 0 {
		return operation.Reject(operation.Invalid, "validation")
	}
	scope := map[string]any{}
	var err error
	if len(in.Scope) != 0 {
		scope, err = ValidatedConnectorScope(d, in.Scope)
		if err != nil {
			return operation.Reject(operation.Invalid, "invalid_scope")
		}
	}
	if s.factories[id] == nil {
		return operation.Reject(operation.Unavailable, "connector_unavailable")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	var result attemptRecord
	err = s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if s.unavailable || s.files.Available() != nil {
			return operation.Fail(operation.Unavailable, "integrations_unavailable")
		}
		if prior, ok := s.state.Attempts[in.OperationID]; ok {
			if prior.ClientID != p.ClientID() || prior.PersonID != p.PersonID() || prior.DeviceID != p.DeviceID() || prior.ConnectorID != id || prior.CatalogRevision != in.ExpectedCatalogRevision || !reflect.DeepEqual(prior.RequestedScope, scope) {
				return operation.Fail(operation.Conflict, "operation_conflict")
			}
			result = prior
			return nil
		}
		if s.state.Revision != in.ExpectedCatalogRevision {
			return operation.Fail(operation.Conflict, "catalog_changed")
		}
		if err := s.readyLocked(p.PersonID()); err != nil {
			return err
		}
		if len(s.state.Attempts) >= 64 {
			return operation.Fail(operation.Limited, "attempt_limit")
		}
		for _, r := range s.state.Connections {
			if r.PersonID == p.PersonID() && r.ConnectorID == id {
				return operation.Fail(operation.Conflict, "already_connected")
			}
		}
		for _, a := range s.state.Attempts {
			if a.PersonID == p.PersonID() && a.ConnectorID == id && (a.Status == Pending || a.Status == AwaitingUser) {
				return operation.Fail(operation.Conflict, "connection_in_progress")
			}
		}
		r := Record{ConnectionID: trust.NewID(), Revision: 1, ConnectorID: id, PersonID: p.PersonID(), Scope: scope, Incarnation: trust.NewID(), Epoch: 1, IdentityUnverified: true}
		namespace := d.CredentialName
		if namespace == "" {
			namespace = d.OAuthCredential
		}
		var err error
		r.Credential, err = credentials.ConnectionName(namespace, r.ConnectionID, r.PersonID)
		if err != nil {
			return err
		}
		result = attemptRecord{ID: in.OperationID, ClientID: p.ClientID(), PersonID: p.PersonID(), DeviceID: p.DeviceID(), ConnectorID: id, Record: r, Status: AwaitingUser, CreatedAt: time.Now().UnixMilli(), Revision: 1, CatalogRevision: in.ExpectedCatalogRevision, RequestedScope: CloneConnectorScope(scope)}
		next := clone(s.state)
		next.Attempts[result.ID] = result
		return s.persist(next)
	})
	if err != nil {
		return trust.Result(err)
	}
	return operation.Accept(attemptResponse(result))
}
func (s *Service) Poll(ctx context.Context, p trust.Principal, id, attemptID string) operation.Result {
	if err := s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error { return nil }); err != nil {
		return trust.Result(err)
	}
	s.mu.RLock()
	a, ok := s.state.Attempts[attemptID]
	denied := s.unavailable
	s.mu.RUnlock()
	if !ok || a.ClientID != p.ClientID() || a.PersonID != p.PersonID() || a.DeviceID != p.DeviceID() || a.ConnectorID != id {
		return operation.Reject(operation.Missing, "attempt_not_found")
	}
	if denied {
		return operation.Reject(operation.Unavailable, "integrations_unavailable")
	}
	return operation.Accept(attemptResponse(a))
}
func (s *Service) Cancel(ctx context.Context, p trust.Principal, id, attemptID string, in CancelSetupRequest) operation.Result {
	if in.SchemaVersion != 1 || in.OperationID != attemptID || in.ExpectedRevision == 0 {
		return operation.Reject(operation.Invalid, "validation")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	var removed attemptRecord
	err := s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if s.unavailable || s.files.Available() != nil {
			return operation.Fail(operation.Unavailable, "integrations_unavailable")
		}
		a, ok := s.state.Attempts[attemptID]
		if !ok || a.ClientID != p.ClientID() || a.PersonID != p.PersonID() || a.DeviceID != p.DeviceID() || a.ConnectorID != id {
			return operation.Fail(operation.Missing, "attempt_not_found")
		}
		if a.Status == Cancelled {
			if in.ExpectedRevision+1 != a.Revision {
				return operation.Fail(operation.Conflict, "operation_changed")
			}
			removed = a
			return nil
		}
		if a.Revision != in.ExpectedRevision {
			return operation.Fail(operation.Conflict, "operation_changed")
		}
		if a.Status != Pending && a.Status != AwaitingUser {
			return operation.Fail(operation.Conflict, "attempt_not_pending")
		}
		next := clone(s.state)
		a.Status = Cancelled
		a.Revision++
		a.AuthorizationURL = ""
		a.UserCode = ""
		next.Attempts[a.ID] = a
		if a.Started {
			queueCleanup(&next, []Record{a.Record}, nil)
		}
		removed = a
		return s.persist(next)
	})
	if err != nil {
		return trust.Result(err)
	}
	_ = s.ResumeCleanup(ctx)
	return operation.Accept(attemptResponse(removed))
}
func (s *Service) UpdateScope(ctx context.Context, p trust.Principal, id string, in ScopeRequest) operation.Result {
	d, ok := DefinitionFor(id)
	if !ok {
		return operation.Reject(operation.Missing, "connector_not_found")
	}
	scope, err := ValidatedConnectorScope(d, in.Scope)
	if err != nil || in.SchemaVersion != 1 {
		return operation.Reject(operation.Invalid, "invalid_scope")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	var result Record
	changedScope := false
	err = s.commit(p, func(st *diskState) error {
		r, ok := st.Connections[in.ConnectionID]
		if !ok || r.PersonID != p.PersonID() || r.ConnectorID != id || r.Revision != in.ConnectionRevision {
			return operation.Fail(operation.Conflict, "connection_changed")
		}
		if !reflect.DeepEqual(r.Scope, scope) {
			changedScope = true
			if r.Epoch == ^uint64(0) || r.Revision == ^uint64(0) {
				return operation.Fail(operation.Conflict, "connection_changed")
			}
			r.Scope = scope
			r.Revision++
			r.Epoch++
			st.Connections[r.ConnectionID] = r
		}
		result = r
		return nil
	})
	if err != nil {
		return trust.Result(err)
	}
	if !changedScope {
		return operation.Accept(map[string]any{"schema_version": 1, "person_id": p.PersonID(), "device_id": p.DeviceID(), "connection_id": result.ConnectionID, "connection_revision": result.Revision, "connector_id": id, "scope": result.Scope})
	}
	// Replace the read runtime after durable scope CAS. Captured old Readers cannot pass the source epoch fence.
	factory := s.factories[id]
	if factory == nil {
		return operation.Reject(operation.Unavailable, "connector_unavailable")
	}
	runtime, err := factory.Open(ctx, RuntimeConfig{cloneRecord(result), binding(result)})
	if err != nil {
		return operation.Reject(operation.Unavailable, "connector_unavailable")
	}
	s.mu.Lock()
	old := s.runtimes[result.ConnectionID]
	s.runtimes[result.ConnectionID] = runtime
	s.mu.Unlock()
	if old.Close != nil {
		old.Close()
	}
	return operation.Accept(map[string]any{"schema_version": 1, "person_id": p.PersonID(), "device_id": p.DeviceID(), "connection_id": result.ConnectionID, "connection_revision": result.Revision, "connector_id": id, "scope": result.Scope})
}
func (s *Service) Disconnect(ctx context.Context, p trust.Principal, id string, in DisconnectRequest) operation.Result {
	if in.SchemaVersion != 1 || !trust.ValidID(in.OperationID) || !trust.ValidID(in.ConnectionID) || in.ConnectionRevision == 0 {
		return operation.Reject(operation.Invalid, "validation")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	request := disconnectOperation{OperationID: in.OperationID, ClientID: p.ClientID(), PersonID: p.PersonID(), DeviceID: p.DeviceID(), ConnectorID: id, ConnectionID: in.ConnectionID, ConnectionRevision: in.ConnectionRevision, CleanupState: "pending"}
	// Replay admission checks current trust, then the durable operation before source existence.
	err := s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if s.unavailable || s.files.Available() != nil {
			return operation.Fail(operation.Unavailable, "integrations_unavailable")
		}
		if prior, ok := s.state.Disconnects[in.OperationID]; ok {
			comparison := prior
			comparison.CleanupState = "pending"
			if comparison != request {
				return operation.Fail(operation.Conflict, "operation_conflict")
			}
			return nil
		}
		if err := s.readyLocked(p.PersonID()); err != nil {
			return err
		}
		if len(s.state.Disconnects) >= 256 {
			return operation.Fail(operation.Limited, "operation_limit")
		}
		r, ok := s.state.Connections[in.ConnectionID]
		if !ok || r.PersonID != p.PersonID() || r.ConnectorID != id || r.Revision != in.ConnectionRevision || r.Device != nil && r.Device.DeviceID != p.DeviceID() {
			return operation.Fail(operation.Conflict, "connection_changed")
		}
		next := clone(s.state)
		delete(next.Connections, r.ConnectionID)
		for key, a := range next.Attempts {
			if a.Record.ConnectionID == r.ConnectionID && a.Status != Connected {
				delete(next.Attempts, key)
			}
		}
		next.Cleanup[in.OperationID] = cleanupRecord{ID: in.OperationID, Records: []Record{r}, RuntimeDone: map[string]bool{}, VaultDone: map[string]bool{}}
		next.Disconnects[in.OperationID] = request
		return s.persist(next)
	})
	if err != nil {
		return trust.Result(err)
	}
	_ = s.ResumeCleanup(ctx)
	s.mu.RLock()
	receipt, exists := s.state.Disconnects[in.OperationID]
	denied := s.unavailable
	s.mu.RUnlock()
	if !exists || denied {
		return operation.Reject(operation.Unavailable, "cleanup_receipt_unavailable")
	}
	return operation.Accept(map[string]any{"schema_version": 1, "operation_id": receipt.OperationID, "person_id": receipt.PersonID, "device_id": receipt.DeviceID, "connection_id": receipt.ConnectionID, "connector_id": receipt.ConnectorID, "connection_revision": receipt.ConnectionRevision, "cleanup_state": receipt.CleanupState})
}
func attemptResponse(a attemptRecord) map[string]any {
	return map[string]any{"schema_version": 1, "operation_id": a.ID, "connector_id": a.ConnectorID, "connection_id": a.Record.ConnectionID, "person_id": a.PersonID, "device_id": a.DeviceID, "setup_state": string(a.Status), "management_ref": "/manage/setup/" + a.ID, "revision": a.Revision}
}
func copyRuntimes(in map[string]Runtime) map[string]Runtime {
	out := map[string]Runtime{}
	for k, v := range in {
		out[k] = v
	}
	return out
}
func queueCleanup(st *diskState, records []Record, t *trust.CleanupTicket) string {
	id := trust.NewID()
	if t != nil {
		id = t.ID
	}
	st.Cleanup[id] = cleanupRecord{id, t, records, map[string]bool{}, map[string]bool{}}
	return id
}
func (s *Service) ApplyRevocation(ctx context.Context, t trust.CleanupTicket) error {
	pending, err := s.trust.PendingCleanup()
	if err != nil {
		return err
	}
	valid := false
	for _, v := range pending {
		valid = valid || v == t
	}
	if !valid {
		return operation.Fail(operation.Conflict, "cleanup_conflict")
	}
	s.mu.Lock()
	if s.unavailable || s.files.Available() != nil {
		s.mu.Unlock()
		return operation.Fail(operation.Unavailable, "integrations_unavailable")
	}
	if receipt, ok := s.state.Receipts[t.ID]; ok {
		s.mu.Unlock()
		return s.trust.AcknowledgeCleanup(ctx, t, receipt)
	}
	if _, ok := s.state.Cleanup[t.ID]; !ok {
		next := clone(s.state)
		records := map[string]Record{}
		for id, a := range next.Attempts {
			if a.ClientID == t.ClientID || t.Kind == trust.PersonAllSources && a.PersonID == t.PersonID {
				if a.Status == Pending {
					records[a.Record.ConnectionID] = a.Record
				}
				delete(next.Attempts, id)
			}
		}
		if t.Kind == trust.PersonAllSources {
			for id, r := range next.Connections {
				if r.PersonID == t.PersonID {
					records[id] = r
					delete(next.Connections, id)
				}
			}
		}
		all := []Record{}
		for _, r := range records {
			all = append(all, r)
		}
		queueCleanup(&next, all, &t)
		if err = s.persist(next); err != nil {
			s.mu.Unlock()
			return err
		}
	}
	s.mu.Unlock()
	return nil
}
func (s *Service) ResumeCleanup(ctx context.Context) error {
	s.mu.RLock()
	denied := s.unavailable
	s.mu.RUnlock()
	if denied {
		return errors.New("integration persistence uncertain")
	}
	tickets, err := s.trust.PendingCleanup()
	if err != nil {
		return err
	}
	for _, t := range tickets {
		if err = s.ApplyRevocation(ctx, t); err != nil {
			return err
		}
	}
	s.mu.RLock()
	ids := make([]string, 0, len(s.state.Cleanup))
	for id := range s.state.Cleanup {
		ids = append(ids, id)
	}
	s.mu.RUnlock()
	sort.Strings(ids)
	for _, id := range ids {
		unlock := s.lock("cleanup/" + id)
		err = s.cleanup(ctx, id)
		unlock()
		if err != nil {
			return err
		}
	}
	return nil
}
func (s *Service) cleanup(ctx context.Context, id string) error {
	s.mu.RLock()
	c, ok := s.state.Cleanup[id]
	s.mu.RUnlock()
	if !ok {
		return nil
	}
	for _, r := range c.Records {
		if err := ctx.Err(); err != nil {
			return err
		}
		unlock := s.lock("cleanup-source/" + r.ConnectionID)
		if !c.RuntimeDone[r.ConnectionID] {
			s.mu.RLock()
			runtime, exists := s.runtimes[r.ConnectionID]
			s.mu.RUnlock()
			if !exists {
				f := s.factories[r.ConnectorID]
				if f == nil {
					unlock()
					return errors.New("cleanup runtime unavailable")
				}
				var err error
				runtime, err = f.Open(ctx, RuntimeConfig{cloneRecord(r), binding(r)})
				if err != nil {
					unlock()
					return err
				}
				s.mu.Lock()
				s.runtimes[r.ConnectionID] = runtime
				s.mu.Unlock()
			}
			if runtime.Setup == nil {
				unlock()
				return errors.New("cleanup setup unavailable")
			}
			if err := runtime.Setup.Disconnect(ctx, binding(r)); err != nil {
				unlock()
				return err
			}
			if runtime.Cleanup != nil {
				if err := runtime.Cleanup(ctx); err != nil {
					unlock()
					return err
				}
			}
			if err := s.cleanupProgress(id, r.ConnectionID, true, false); err != nil {
				unlock()
				return err
			}
			c.RuntimeDone[r.ConnectionID] = true
		}
		if !c.VaultDone[r.ConnectionID] {
			if r.Credential != "" {
				if err := s.vault.Delete(ctx, r.Credential); err != nil {
					unlock()
					return err
				}
			}
			if err := s.cleanupProgress(id, r.ConnectionID, true, true); err != nil {
				unlock()
				return err
			}
			c.VaultDone[r.ConnectionID] = true
		}
		s.mu.Lock()
		runtime := s.runtimes[r.ConnectionID]
		delete(s.runtimes, r.ConnectionID)
		s.mu.Unlock()
		if runtime.Close != nil {
			runtime.Close()
		}
		unlock()
	}
	s.mu.Lock()
	next := clone(s.state)
	current, ok := next.Cleanup[id]
	if !ok {
		s.mu.Unlock()
		return nil
	}
	for _, r := range current.Records {
		if !current.RuntimeDone[r.ConnectionID] || !current.VaultDone[r.ConnectionID] {
			s.mu.Unlock()
			return errors.New("cleanup incomplete")
		}
	}
	delete(next.Cleanup, id)
	if operation, exists := next.Disconnects[id]; exists {
		operation.CleanupState = "completed"
		next.Disconnects[id] = operation
	}
	var receipt trust.CleanupReceipt
	if current.Ticket != nil {
		t := *current.Ticket
		receipt = trust.CleanupReceipt{TicketID: t.ID, TicketRevision: t.Revision, TrustGeneration: t.TrustGeneration, IntegrationRevision: s.state.Revision + 1}
		next.Receipts[id] = receipt
	}
	err := s.persist(next)
	s.mu.Unlock()
	if err != nil {
		return err
	}
	if current.Ticket != nil {
		return s.trust.AcknowledgeCleanup(ctx, *current.Ticket, receipt)
	}
	return nil
}
func (s *Service) cleanupProgress(id, connection string, runtime, vault bool) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	next := clone(s.state)
	c, ok := next.Cleanup[id]
	if !ok {
		return errors.New("cleanup changed")
	}
	c.RuntimeDone[connection] = c.RuntimeDone[connection] || runtime
	c.VaultDone[connection] = c.VaultDone[connection] || vault
	next.Cleanup[id] = c
	return s.persist(next)
}
