package connections

import (
	"context"
	"errors"
	"testing"
)

// The listing must keep its ownership, device-binding and failure semantics.

type source struct {
	connector string
	device    string
	err       error
}

func (s source) ConnectionSnapshot(context.Context) (any, error) {
	if s.err != nil {
		return nil, s.err
	}
	value := map[string]any{"connection": map[string]any{"connector_id": s.connector}}
	if s.device != "" {
		value["descriptor"] = map[string]any{"execution": map[string]any{"kind": "device", "device_id": s.device}}
	}
	return value, nil
}

type owner map[string]Record

func (o owner) OwnedConnection(connectorID, personID string) (Record, string, bool) {
	record, ok := o[connectorID]
	if !ok || record.PersonID != personID {
		return Record{}, "", false
	}
	return record, "owner-1", true
}

func TestConnectionListingOwnershipAndFailures(t *testing.T) {
	scope := Scope{PersonID: "p1", DeviceID: "d1"}
	records := map[string]Record{"c1": {ConnectionID: "c1", ConnectorID: "gmail", PersonID: "p1"}}
	if !OwnedByConnector(records, "gmail", scope) || OwnedByConnector(records, "other", scope) {
		t.Fatal("connector ownership")
	}
	if !OwnedByConnection(records, "c1", scope) || OwnedByConnection(records, "c2", scope) {
		t.Fatal("connection ownership")
	}
	bound := map[string]Record{"c1": {ConnectionID: "c1", ConnectorID: "gmail", PersonID: "p1", Device: &deviceBinding{DeviceID: "other"}}}
	if OwnedByConnection(bound, "c1", scope) {
		t.Fatal("device-bound connection leaked to another device")
	}

	sources := Sources{ByConnector: map[string]SnapshotSource{"gmail": source{connector: "gmail"}}}
	out, err := List(context.Background(), scope, records, sources, owner{"gmail": records["c1"]})
	if err != nil || len(out) != 1 {
		t.Fatalf("listing: %v %v", out, err)
	}
	connection := out[0].(map[string]any)["connection"].(map[string]any)
	if connection["connection_id"] != "c1" || connection["authority"].(map[string]any)["execution_owner"] != "owner-1" {
		t.Fatalf("stamp: %v", connection)
	}

	failing := Sources{ByConnector: map[string]SnapshotSource{"gmail": source{connector: "gmail", err: errors.New("down")}}}
	if _, err := List(context.Background(), scope, records, failing, owner{"gmail": records["c1"]}); err == nil {
		t.Fatal("a runtime failure must fail the listing")
	}

	mismatch := Sources{ByConnector: map[string]SnapshotSource{"gmail": source{connector: "gmail", device: "d9"}}}
	if _, err := List(context.Background(), scope, records, mismatch, owner{"gmail": records["c1"]}); !errors.Is(err, ErrScopeUnavailable) {
		t.Fatal("a device mismatch must fail the scope")
	}

	unowned := Sources{ByConnector: map[string]SnapshotSource{"gmail": source{connector: "gmail"}}}
	out, err = List(context.Background(), scope, records, unowned, owner{})
	if err != nil || len(out) != 0 {
		t.Fatalf("unrecorded connector must be dropped: %v %v", out, err)
	}
}
