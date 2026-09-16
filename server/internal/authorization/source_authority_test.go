package authorization

import (
	"testing"

	"floe/server/internal/authorization"
)

func TestCurrentSourceFencesEpochOwnerIdentityAndDevice(t *testing.T) {
	fixture := setup(t)
	clientID, _ := fixture.pair()
	connectionID := fixtureConnectionID("gmail")
	fixture.console.mu.Lock()
	fixture.console.state.Connections[connectionID] = connectionRecord{ConnectionID: connectionID, Revision: 1, ConnectorID: "gmail", PersonID: fixturePersonID, Incarnation: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", Epoch: 2, ProviderIdentity: "provider-subject", Device: &deviceBinding{DeviceID: fixtureDeviceID}}
	executionOwner := fixture.console.state.ExecutionOwnerID
	fixture.console.mu.Unlock()
	principal := authorization.Principal{ClientID: clientID, PersonID: fixturePersonID, DeviceID: fixtureDeviceID, Authenticated: true}
	reference := authorization.SourceReference{ConnectorID: "gmail", ConnectionID: connectionID, ExecutionOwner: executionOwner, Incarnation: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", Epoch: 2}
	called := false
	if err := fixture.console.WithCurrentSource(principal, reference, func(snapshot authorization.SourceSnapshot) error { called = snapshot.Active; return nil }); err != nil || !called {
		t.Fatalf("source rejected: %v", err)
	}
	wrongEpoch := reference
	wrongEpoch.Epoch = 3
	if err := fixture.console.WithCurrentSource(principal, wrongEpoch, func(authorization.SourceSnapshot) error { return nil }); err == nil {
		t.Fatal("wrong epoch accepted")
	}
	wrongOwner := reference
	wrongOwner.ExecutionOwner = "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
	if err := fixture.console.WithCurrentSource(principal, wrongOwner, func(authorization.SourceSnapshot) error { return nil }); err == nil {
		t.Fatal("wrong owner accepted")
	}
	fixture.console.mu.Lock()
	record := fixture.console.state.Connections[connectionID]
	record.IdentityUnverified = true
	fixture.console.state.Connections[connectionID] = record
	fixture.console.mu.Unlock()
	if err := fixture.console.WithCurrentSource(principal, reference, func(authorization.SourceSnapshot) error { return nil }); err == nil {
		t.Fatal("unverified identity accepted")
	}
}
