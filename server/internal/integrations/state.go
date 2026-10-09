package integrations

import (
	"errors"
	"floe/server/internal/trust"
	"reflect"
)

func initialState() StateSnapshot {
	return StateSnapshot{1, 1, map[string]Record{}, map[string]AttemptSnapshot{}, map[string]CleanupSnapshot{}, map[string]trust.CleanupReceipt{}, map[string]DisconnectSnapshot{}}
}
func clone(st StateSnapshot) StateSnapshot {
	out := initialState()
	out.Revision = st.Revision
	for k, r := range st.Connections {
		out.Connections[k] = cloneRecord(r)
	}
	for k, a := range st.Attempts {
		a.Record = cloneRecord(a.Record)
		a.RequestedScope = CloneConnectorScope(a.RequestedScope)
		out.Attempts[k] = a
	}
	for k, c := range st.Cleanup {
		c.Records = append([]Record(nil), c.Records...)
		for i := range c.Records {
			c.Records[i] = cloneRecord(c.Records[i])
		}
		rd, vd := map[string]bool{}, map[string]bool{}
		for k, v := range c.RuntimeDone {
			rd[k] = v
		}
		for k, v := range c.VaultDone {
			vd[k] = v
		}
		c.RuntimeDone, c.VaultDone = rd, vd
		if c.Ticket != nil {
			t := *c.Ticket
			c.Ticket = &t
		}
		out.Cleanup[k] = c
	}
	for k, v := range st.Receipts {
		out.Receipts[k] = v
	}
	for k, v := range st.Disconnects {
		out.Disconnects[k] = v
	}
	return out
}
func cloneRecord(r Record) Record {
	r.Scope = CloneConnectorScope(r.Scope)
	if r.Device != nil {
		d := *r.Device
		r.Device = &d
	}
	return r
}
func binding(r Record) CredentialBinding {
	return CredentialBinding{r.Credential, r.ConnectionID, r.PersonID, r.Incarnation, 1}
}
func validateRecord(r Record) bool {
	d, ok := DefinitionFor(r.ConnectorID)
	if !ok || !trust.ValidID(r.ConnectionID) || !trust.ValidID(r.PersonID) || !trust.ValidID(r.Incarnation) || r.Revision == 0 || r.Epoch == 0 || r.Device != nil && !trust.ValidDevice(r.Device.DeviceID) {
		return false
	}
	if canonical, err := ValidatedConnectorScope(d, r.Scope); err != nil || !reflect.DeepEqual(canonical, CloneConnectorScope(r.Scope)) {
		return false
	}
	namespace := d.CredentialName
	if namespace == "" {
		namespace = d.OAuthCredential
	}
	slot, err := CredentialSlot(namespace, r.ConnectionID, r.PersonID)
	return err == nil && slot == r.Credential
}

func validateState(st StateSnapshot) error {
	if st.SchemaVersion != 1 || st.Revision == 0 || st.Connections == nil || st.Attempts == nil || st.Cleanup == nil || st.Receipts == nil || st.Disconnects == nil || len(st.Connections) > 9 || len(st.Attempts) > 64 || len(st.Cleanup) > 64 || len(st.Receipts) > 128 || len(st.Disconnects) > 256 {
		return errors.New("integration state unavailable")
	}
	owned := map[string]bool{}
	for id, r := range st.Connections {
		key := r.PersonID + "/" + r.ConnectorID
		if id != r.ConnectionID || !validateRecord(r) || owned[key] {
			return errors.New("invalid integration state")
		}
		owned[key] = true
	}
	for id, a := range st.Attempts {
		if id != a.ID || !trust.ValidID(id) || !trust.ValidID(a.ClientID) || a.PersonID != a.Record.PersonID || !trust.ValidDevice(a.DeviceID) || !(validateRecord(a.Record) || (a.Status == AwaitingUser || a.Status == Cancelled || a.Status == Failed) && !a.Started && len(a.Record.Scope) == 0 && validAttemptIdentity(a.Record)) || a.RequestedScope == nil || a.CatalogRevision == 0 || a.ConnectorID != a.Record.ConnectorID || a.CreatedAt <= 0 || a.Revision == 0 || a.Status != Pending && a.Status != Connected && a.Status != Failed && a.Status != AwaitingUser && a.Status != Cancelled {
			return errors.New("invalid integration attempt")
		}
	}
	for _, attempt := range st.Attempts {
		if len(attempt.RequestedScope) > 0 {
			definition, ok := DefinitionFor(attempt.ConnectorID)
			canonical, err := ValidatedConnectorScope(definition, attempt.RequestedScope)
			if !ok || err != nil || !reflect.DeepEqual(canonical, CloneConnectorScope(attempt.RequestedScope)) {
				return errors.New("invalid integration request identity")
			}
		}
	}
	for id, c := range st.Cleanup {
		if id != c.ID || !trust.ValidID(id) || c.RuntimeDone == nil || c.VaultDone == nil {
			return errors.New("invalid integration cleanup")
		}
		for _, r := range c.Records {
			if !validateRecord(r) {
				return errors.New("invalid integration cleanup")
			}
		}
	}
	for id, o := range st.Disconnects {
		if id != o.OperationID || !trust.ValidID(id) || !trust.ValidID(o.ClientID) || !trust.ValidID(o.PersonID) || !trust.ValidDevice(o.DeviceID) || !trust.ValidID(o.ConnectionID) || o.ConnectionRevision == 0 || o.CleanupState != "pending" && o.CleanupState != "completed" {
			return errors.New("invalid disconnect operation")
		}
	}
	return nil
}

func validAttemptIdentity(r Record) bool {
	d, ok := DefinitionFor(r.ConnectorID)
	if !ok || !trust.ValidID(r.ConnectionID) || !trust.ValidID(r.PersonID) || !trust.ValidID(r.Incarnation) || r.Revision != 1 || r.Epoch != 1 {
		return false
	}
	namespace := d.CredentialName
	if namespace == "" {
		namespace = d.OAuthCredential
	}
	slot, err := CredentialSlot(namespace, r.ConnectionID, r.PersonID)
	return err == nil && slot == r.Credential
}
