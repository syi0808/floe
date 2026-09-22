package connections

import (
	"context"
	"encoding/json"
	"errors"
)

// Listing the connections one authenticated client may see.
//
// Ownership is decided here, not at the transport: a connector runtime is only
// reported when this Person owns a connection to it, and a device-bound
// connection is only reported to the device it was bound to. Transport turns
// the result into a response and never re-derives it.

// Scope is the authenticated client a listing is made for.
type Scope struct {
	ClientID string
	PersonID string
	DeviceID string
}

// SnapshotSource is one connector runtime that can report its own connection.
type SnapshotSource interface {
	ConnectionSnapshot(context.Context) (any, error)
}

// Sources names the runtimes this server may report on, keyed by how the
// connection behind each one is identified.
type Sources struct {
	// ByConnector holds runtimes reported when the Person owns that connector.
	ByConnector map[string]SnapshotSource
	// ByConnection holds runtimes reported when the Person owns that connection.
	ByConnection map[string]SnapshotSource
}

// OwnershipReader resolves the recorded connection behind a reported snapshot.
//
// The record and the execution authority are read together so a snapshot is
// never stamped with an authority that no longer owns it.
type OwnershipReader interface {
	OwnedConnection(connectorID, personID string) (record Record, executionOwner string, exists bool)
}

// ErrScopeUnavailable means a reported snapshot could not be attributed to a
// connection this client owns. A listing fails rather than reporting a
// connection under the wrong owner or device.
var ErrScopeUnavailable = errors.New("connection ownership unavailable")

// OwnedByConnector reports whether this client owns a connection to a connector.
func OwnedByConnector(records map[string]Record, connectorID string, scope Scope) bool {
	for _, record := range records {
		if record.ConnectorID == connectorID && record.PersonID == scope.PersonID && (record.Device == nil || record.Device.DeviceID == scope.DeviceID) {
			return true
		}
	}
	return false
}

// OwnedByConnection reports whether this client owns one recorded connection.
func OwnedByConnection(records map[string]Record, connectionID string, scope Scope) bool {
	record, exists := records[connectionID]
	return exists && record.PersonID == scope.PersonID && (record.Device == nil || record.Device.DeviceID == scope.DeviceID)
}

// List collects the connection snapshots this client may see.
//
// Runtimes are selected first, so no snapshot is ever fetched for a connection
// the client does not own; a runtime that cannot report is a failure for the
// whole listing rather than a silently shorter list.
func List(ctx context.Context, scope Scope, records map[string]Record, sources Sources, ownership OwnershipReader) ([]any, error) {
	selected := make([]SnapshotSource, 0, len(sources.ByConnector)+len(sources.ByConnection))
	for connectorID, runtime := range sources.ByConnector {
		if runtime != nil && OwnedByConnector(records, connectorID, scope) {
			selected = append(selected, runtime)
		}
	}
	for connectionID, runtime := range sources.ByConnection {
		if runtime != nil && OwnedByConnection(records, connectionID, scope) {
			selected = append(selected, runtime)
		}
	}
	snapshots := make([]any, 0, len(selected))
	for _, runtime := range selected {
		snapshot, err := runtime.ConnectionSnapshot(ctx)
		if err != nil {
			return nil, err
		}
		snapshots = append(snapshots, snapshot)
	}
	return ownedSnapshots(snapshots, scope, ownership)
}

// ownedSnapshots stamps each reported snapshot with the connection identity this
// server recorded for it, dropping connectors this Person does not own.
func ownedSnapshots(snapshots []any, scope Scope, ownership OwnershipReader) ([]any, error) {
	owned := make([]any, 0, len(snapshots))
	for _, snapshot := range snapshots {
		value, connectorID, deviceID, ok := SnapshotMetadata(snapshot)
		if !ok {
			return nil, ErrScopeUnavailable
		}
		record, executionOwner, exists := ownership.OwnedConnection(connectorID, scope.PersonID)
		if !exists {
			continue
		}
		if record.Device != nil && (record.Device.DeviceID != scope.DeviceID || record.Device.DeviceID != deviceID) || record.Device == nil && deviceID != "" {
			return nil, ErrScopeUnavailable
		}
		connection := value["connection"].(map[string]any)
		connection["person_id"] = record.PersonID
		connection["connection_id"] = record.ConnectionID
		if record.Device != nil {
			connection["device_binding"] = map[string]any{"device_id": record.Device.DeviceID}
		}
		connection["authority"] = map[string]any{"execution_owner": executionOwner, "incarnation": record.Incarnation, "epoch": record.Epoch, "identity_unverified": record.IdentityUnverified}
		owned = append(owned, value)
	}
	return owned, nil
}

// LegacySnapshotSource adapts a runtime whose snapshot takes no context.
type LegacySnapshotSource struct {
	Runtime interface {
		ConnectionSnapshot() (any, error)
	}
}

func (source LegacySnapshotSource) ConnectionSnapshot(context.Context) (any, error) {
	return source.Runtime.ConnectionSnapshot()
}

// SnapshotMetadata reads the connector and device a reported snapshot claims.
func SnapshotMetadata(snapshot any) (map[string]any, string, string, bool) {
	encoded, err := json.Marshal(snapshot)
	if err != nil {
		return nil, "", "", false
	}
	var value map[string]any
	if json.Unmarshal(encoded, &value) != nil {
		return nil, "", "", false
	}
	connection, ok := value["connection"].(map[string]any)
	if !ok {
		return nil, "", "", false
	}
	connectorID, ok := connection["connector_id"].(string)
	if !ok || connectorID == "" {
		return nil, "", "", false
	}
	deviceID := ""
	if descriptor, ok := value["descriptor"].(map[string]any); ok {
		if execution, ok := descriptor["execution"].(map[string]any); ok && execution["kind"] == "device" {
			deviceID, _ = execution["device_id"].(string)
		}
	}
	return value, connectorID, deviceID, true
}

// SnapshotConnectorID reads which connector a reported snapshot belongs to.
func SnapshotConnectorID(snapshot any) (string, bool) {
	_, connectorID, _, ok := SnapshotMetadata(snapshot)
	return connectorID, ok
}
