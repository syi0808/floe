// Package integrations owns source lifecycle, resource CAS and durable cleanup.
package integrations

import (
	"context"
	"errors"
	"floe/server/internal/operation"
	"floe/server/internal/trust"
	"reflect"
	"sort"
	"sync"
	"time"
)

type Service struct {
	mu          sync.RWMutex
	repository  Repository
	trust       Trust
	credentials CredentialAccess
	factories   map[string]RuntimeFactory
	state       StateSnapshot
	runtimes    map[string]Runtime
	unavailable bool
	lifecycles  sync.Map
}

func New(ctx context.Context, repository Repository, t Trust, credentials CredentialAccess, factories map[string]RuntimeFactory) (*Service, error) {
	if repository == nil || t == nil || credentials == nil {
		return nil, errors.New("integration dependency unavailable")
	}
	loaded := repository.LoadState()
	var state StateSnapshot
	switch loaded.Disposition {
	case LoadAbsent:
		state = initialState()
	case LoadPresent:
		state = loaded.Snapshot
	default:
		return nil, errors.New("integration state unavailable")
	}
	if err := validateState(state); err != nil {
		return nil, err
	}
	s := &Service{repository: repository, trust: t, credentials: credentials, factories: map[string]RuntimeFactory{}, state: clone(state), runtimes: map[string]Runtime{}}
	for k, f := range factories {
		s.factories[k] = f
	}
	// Interrupted authorization never resumes with a provider: journal cleanup before opening sources.
	for id, a := range s.state.Attempts {
		if a.Status == Pending {
			c := CleanupSnapshot{ID: trust.NewID(), Records: []Record{a.Record}, RuntimeDone: map[string]bool{}, VaultDone: map[string]bool{}}
			s.state.Cleanup[c.ID] = c
			a.Status = Failed
			a.Revision++
			a.ErrorCode = "authorization_interrupted"
			a.AuthorizationURL = ""
			a.UserCode = ""
			s.state.Attempts[id] = a
		}
	}
	if err := s.persist(s.state); err != nil {
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
	if !s.storageReadyLocked() {
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
func (s *Service) storageReadyLocked() bool {
	return !s.unavailable && s.repository.Health() == RepositoryReady
}
func (s *Service) persist(st StateSnapshot) error {
	if !s.storageReadyLocked() {
		return errors.New("integration persistence unavailable")
	}
	st.Revision = s.state.Revision + 1
	outcome := s.repository.SaveState(st)
	switch outcome.Disposition {
	case WriteCommitted:
		s.state = clone(st)
		return nil
	case WriteIndeterminate, WriteIntegrityFailure:
		s.unavailable = true
	}
	return errors.New("integration persistence unavailable")
}
func (s *Service) commit(p trust.Principal, mutate func(*StateSnapshot) error) error {
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
func (s *Service) Catalog(ctx context.Context, p trust.Principal) (CatalogResult, error) {
	if err := s.check(p); err != nil {
		return CatalogResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	s.mu.RLock()
	state := clone(s.state)
	runtimes := copyRuntimes(s.runtimes)
	s.mu.RUnlock()
	meta, err := s.trust.ProducerMetadata()
	if err != nil {
		return CatalogResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	items := make([]ConnectorCatalogEntry, 0, len(Definitions()))
	for _, d := range Definitions() {
		item := ConnectorCatalogEntry{ID: d.ID, Name: d.Name, AuthKind: d.AuthKind, Available: s.factories[d.ID] != nil, Status: "disconnected", RequiredScopes: d.RequiredScopes, ScopeFields: d.ScopeFields, Capabilities: CapabilitiesFor(d)}
		if s.factories[d.ID] == nil {
			item.Status = "unavailable"
		}
		for _, r := range state.Connections {
			if r.PersonID == p.PersonID() && r.ConnectorID == d.ID {
				status := "error"
				if runtime, ok := runtimes[r.ConnectionID]; ok && runtime.Setup != nil && runtime.Setup.CachedStatus(binding(r)).Ready {
					status = "connected"
				}
				item.Status = status
				item.HasConnection = true
				item.ConnectionID = r.ConnectionID
				item.ConnectionRevision = r.Revision
				item.Incarnation = r.Incarnation
				item.Epoch = r.Epoch
				item.ExecutionOwner = meta.ExecutionOwner
				item.IdentityUnverified = r.IdentityUnverified
				item.Scope = r.Scope
			}
		}
		for _, a := range state.Attempts {
			if a.ClientID == p.ClientID() && a.ConnectorID == d.ID && a.Status == Pending {
				item.Status = "connecting"
			}
		}
		items = append(items, item)
	}
	if err = s.check(p); err != nil {
		return CatalogResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	return CatalogResult{SchemaVersion: 1, PersonID: p.PersonID(), DeviceID: p.DeviceID(), Connectors: items, Revision: state.Revision}, nil
}
func (s *Service) Start(ctx context.Context, p trust.Principal, id string, in ConnectRequest) (AttemptResult, error) {
	d, ok := DefinitionFor(id)
	if !ok {
		return AttemptResult{}, operation.Fail(operation.Missing, "connector_not_found")
	}
	if in.SchemaVersion != 1 || !trust.ValidID(in.OperationID) || in.Scope == nil || in.ExpectedCatalogRevision == 0 {
		return AttemptResult{}, operation.Fail(operation.Invalid, "validation")
	}
	scope := map[string]any{}
	var err error
	if len(in.Scope) != 0 {
		scope, err = ValidatedConnectorScope(d, in.Scope)
		if err != nil {
			return AttemptResult{}, operation.Fail(operation.Invalid, "invalid_scope")
		}
	}
	if s.factories[id] == nil {
		return AttemptResult{}, operation.Fail(operation.Unavailable, "connector_unavailable")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	var result AttemptSnapshot
	err = s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if !s.storageReadyLocked() {
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
		r.Credential, err = CredentialSlot(namespace, r.ConnectionID, r.PersonID)
		if err != nil {
			return err
		}
		result = AttemptSnapshot{ID: in.OperationID, ClientID: p.ClientID(), PersonID: p.PersonID(), DeviceID: p.DeviceID(), ConnectorID: id, Record: r, Status: AwaitingUser, CreatedAt: time.Now().UnixMilli(), Revision: 1, CatalogRevision: in.ExpectedCatalogRevision, RequestedScope: CloneConnectorScope(scope)}
		next := clone(s.state)
		next.Attempts[result.ID] = result
		return s.persist(next)
	})
	if err != nil {
		return AttemptResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	return attemptResponse(result), nil
}
func (s *Service) Poll(ctx context.Context, p trust.Principal, id, attemptID string) (AttemptResult, error) {
	if err := s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error { return nil }); err != nil {
		return AttemptResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	s.mu.RLock()
	a, ok := s.state.Attempts[attemptID]
	denied := !s.storageReadyLocked()
	s.mu.RUnlock()
	if !ok || a.ClientID != p.ClientID() || a.PersonID != p.PersonID() || a.DeviceID != p.DeviceID() || a.ConnectorID != id {
		return AttemptResult{}, operation.Fail(operation.Missing, "attempt_not_found")
	}
	if denied {
		return AttemptResult{}, operation.Fail(operation.Unavailable, "integrations_unavailable")
	}
	return attemptResponse(a), nil
}
func (s *Service) Cancel(ctx context.Context, p trust.Principal, id, attemptID string, in CancelSetupRequest) (AttemptResult, error) {
	if in.SchemaVersion != 1 || in.OperationID != attemptID || in.ExpectedRevision == 0 {
		return AttemptResult{}, operation.Fail(operation.Invalid, "validation")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	var removed AttemptSnapshot
	err := s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if !s.storageReadyLocked() {
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
		return AttemptResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	_ = s.ResumeCleanup(ctx)
	return attemptResponse(removed), nil
}
func (s *Service) UpdateScope(ctx context.Context, p trust.Principal, id string, in ScopeRequest) (ScopeResult, error) {
	d, ok := DefinitionFor(id)
	if !ok {
		return ScopeResult{}, operation.Fail(operation.Missing, "connector_not_found")
	}
	scope, err := ValidatedConnectorScope(d, in.Scope)
	if err != nil || in.SchemaVersion != 1 {
		return ScopeResult{}, operation.Fail(operation.Invalid, "invalid_scope")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	var result Record
	changedScope := false
	err = s.commit(p, func(st *StateSnapshot) error {
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
		return ScopeResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	if !changedScope {
		return scopeResult(p, id, result), nil
	}
	// Replace the read runtime after durable scope CAS. Captured old Readers cannot pass the source epoch fence.
	factory := s.factories[id]
	if factory == nil {
		return ScopeResult{}, operation.Fail(operation.Unavailable, "connector_unavailable")
	}
	runtime, err := factory.Open(ctx, RuntimeConfig{cloneRecord(result), binding(result)})
	if err != nil {
		return ScopeResult{}, operation.Fail(operation.Unavailable, "connector_unavailable")
	}
	s.mu.Lock()
	old := s.runtimes[result.ConnectionID]
	s.runtimes[result.ConnectionID] = runtime
	s.mu.Unlock()
	if old.Close != nil {
		old.Close()
	}
	return scopeResult(p, id, result), nil
}
func (s *Service) Disconnect(ctx context.Context, p trust.Principal, id string, in DisconnectRequest) (DisconnectResult, error) {
	if in.SchemaVersion != 1 || !trust.ValidID(in.OperationID) || !trust.ValidID(in.ConnectionID) || in.ConnectionRevision == 0 {
		return DisconnectResult{}, operation.Fail(operation.Invalid, "validation")
	}
	unlock := s.lock(p.PersonID() + "/" + id)
	defer unlock()
	request := DisconnectSnapshot{OperationID: in.OperationID, ClientID: p.ClientID(), PersonID: p.PersonID(), DeviceID: p.DeviceID(), ConnectorID: id, ConnectionID: in.ConnectionID, ConnectionRevision: in.ConnectionRevision, CleanupState: "pending"}
	// Replay admission checks current trust, then the durable operation before source existence.
	err := s.trust.WithCurrentPrincipal(p, func(trust.PrincipalSnapshot) error {
		s.mu.Lock()
		defer s.mu.Unlock()
		if !s.storageReadyLocked() {
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
		next.Cleanup[in.OperationID] = CleanupSnapshot{ID: in.OperationID, Records: []Record{r}, RuntimeDone: map[string]bool{}, VaultDone: map[string]bool{}}
		next.Disconnects[in.OperationID] = request
		return s.persist(next)
	})
	if err != nil {
		return DisconnectResult{}, operation.Normalize(err, operation.Unavailable, "operation_unavailable")
	}
	_ = s.ResumeCleanup(ctx)
	s.mu.RLock()
	receipt, exists := s.state.Disconnects[in.OperationID]
	denied := !s.storageReadyLocked()
	s.mu.RUnlock()
	if !exists || denied {
		return DisconnectResult{}, operation.Fail(operation.Unavailable, "cleanup_receipt_unavailable")
	}
	return DisconnectResult{SchemaVersion: 1, OperationID: receipt.OperationID, PersonID: receipt.PersonID, DeviceID: receipt.DeviceID, ConnectionID: receipt.ConnectionID, ConnectorID: receipt.ConnectorID, ConnectionRevision: receipt.ConnectionRevision, CleanupState: receipt.CleanupState}, nil
}
func attemptResponse(a AttemptSnapshot) AttemptResult {
	return AttemptResult{SchemaVersion: 1, OperationID: a.ID, ConnectorID: a.ConnectorID, ConnectionID: a.Record.ConnectionID, PersonID: a.PersonID, DeviceID: a.DeviceID, SetupState: string(a.Status), ManagementRef: "/manage/setup/" + a.ID, Revision: a.Revision}
}
func scopeResult(p trust.Principal, connectorID string, record Record) ScopeResult {
	return ScopeResult{SchemaVersion: 1, PersonID: p.PersonID(), DeviceID: p.DeviceID(), ConnectionID: record.ConnectionID, ConnectionRevision: record.Revision, ConnectorID: connectorID, Scope: record.Scope}
}
func copyRuntimes(in map[string]Runtime) map[string]Runtime {
	out := map[string]Runtime{}
	for k, v := range in {
		out[k] = v
	}
	return out
}
func queueCleanup(st *StateSnapshot, records []Record, t *trust.CleanupTicket) string {
	id := trust.NewID()
	if t != nil {
		id = t.ID
	}
	st.Cleanup[id] = CleanupSnapshot{id, t, records, map[string]bool{}, map[string]bool{}}
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
	if !s.storageReadyLocked() {
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
	denied := !s.storageReadyLocked()
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
				if err := s.credentials.DeleteConnectionCredential(ctx, binding(r)); err != nil {
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
