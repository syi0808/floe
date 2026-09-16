package authorization

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"
)

func remoteViewResource(viewID, connectionID string) string {
	return viewID + ":" + connectionID
}

type remoteViewAdmissionState struct {
	path          string
	query         []byte
	principal     Principal
	connectorID   string
	connectionID  string
	connectionRev uint64
	expires       time.Time
}

type legacyCommunicationAdapter struct{ runtime ConnectorAuthRuntime }

func snapshotConnectionID(snapshot any) string {
	value, _, _, ok := connectionSnapshotMetadata(snapshot)
	if !ok {
		return ""
	}
	connection, ok := value["connection"].(map[string]any)
	if !ok {
		return ""
	}
	identifier, _ := connection["connection_id"].(string)
	return identifier
}

func (adapter legacyCommunicationAdapter) ConnectionSnapshot(context.Context) (any, error) {
	return adapter.runtime.ConnectionSnapshot()
}

func (adapter legacyCommunicationAdapter) ReadCommunicationView(_ context.Context, query string, cursor, limit int) (any, error) {
	return adapter.runtime.ReadCommunicationView(query, cursor, limit)
}

type remoteViewAdmissionWire struct {
	SchemaVersion      int                `json:"schema_version"`
	ConnectorID        string             `json:"connector_id"`
	ConnectionID       string             `json:"connection_id"`
	ConnectionRevision uint64             `json:"connection_revision"`
	Resources          []string           `json:"resources"`
	Policy             calendarPolicyWire `json:"policy"`
	Grant              calendarGrantWire  `json:"grant"`
	Purpose            string             `json:"purpose"`
	Consumer           string             `json:"consumer"`
	MaxItems           uint32             `json:"max_items"`
	MaxBytes           uint32             `json:"max_bytes"`
	Query              json.RawMessage    `json:"query"`
}

func (console *Console) serveRemoteViewAuthority(
	writer http.ResponseWriter,
	request *http.Request,
	path string,
	principal Principal,
	connectionRecords map[string]connectionRecord,
	communication []CommunicationRuntime,
	work []WorkContextRuntime,
	logistics []LogisticsRuntime,
) {
	if request.Method != http.MethodPost {
		failure(writer, http.StatusMethodNotAllowed, "method_not_allowed")
		return
	}
	switch path {
	case "/v1/views/mail.communication/admit", "/v1/views/work.context/admit", "/v1/views/life.logistics/admit":
		console.serveRemoteViewAdmission(writer, request, path, principal, connectionRecords, communication, work, logistics)
	case "/v1/views/mail.communication/read", "/v1/views/work.context/read", "/v1/views/life.logistics/read":
		console.serveRemoteViewRead(writer, request, path, principal, connectionRecords, communication, work, logistics)
	case "/v1/views/mail.communication/release", "/v1/views/work.context/release", "/v1/views/life.logistics/release":
		console.serveRemoteViewRelease(writer, request, principal)
	default:
		failure(writer, http.StatusNotFound, "not_found")
	}
}

func remoteViewRoute(path string) (string, string, bool) {
	switch path {
	case "/v1/views/mail.communication/admit", "/v1/views/mail.communication/read", "/v1/views/mail.communication/release":
		return "mail.communication", "gmail", true
	case "/v1/views/work.context/admit", "/v1/views/work.context/read", "/v1/views/work.context/release":
		return "work.context", "", true
	case "/v1/views/life.logistics/admit", "/v1/views/life.logistics/read", "/v1/views/life.logistics/release":
		return "life.logistics", "", true
	default:
		return "", "", false
	}
}

func (console *Console) readRemoteView(ctx context.Context, viewID, connectionID, connectorID string, query []byte, communication []CommunicationRuntime, work []WorkContextRuntime, logistics []LogisticsRuntime) ([]byte, uint32, error) {
	var payload struct {
		Query  string `json:"query"`
		Cursor int    `json:"cursor"`
		Limit  int    `json:"limit"`
	}
	if viewID == "mail.communication" {
		decoder := json.NewDecoder(strings.NewReader(string(query)))
		decoder.DisallowUnknownFields()
		if decoder.Decode(&payload) != nil || decoder.Decode(new(any)) != io.EOF || len(payload.Query) > 512 || payload.Cursor < 0 || payload.Limit < 1 || payload.Limit > 100 {
			return nil, 0, errors.New("invalid mail query")
		}
		for _, runtime := range communication {
			if runtime == nil {
				continue
			}
			snapshot, err := runtime.ConnectionSnapshot(ctx)
			if err != nil {
				continue
			}
			_, connector, _, ok := connectionSnapshotMetadata(snapshot)
			if !ok || connector != connectorID || snapshotConnectionID(snapshot) != connectionID {
				continue
			}
			view, err := runtime.ReadCommunicationView(ctx, payload.Query, payload.Cursor, payload.Limit)
			if err == nil {
				encoded, marshalErr := json.Marshal(view)
				return encoded, viewItemCount(encoded), marshalErr
			}
		}
		return nil, 0, errors.New("mail unavailable")
	}
	if len(query) != len(`{"schema_version":1}`) || string(query) != `{"schema_version":1}` {
		return nil, 0, errors.New("invalid view query")
	}
	if viewID == "work.context" {
		for _, runtime := range work {
			if runtime == nil {
				continue
			}
			snapshot, err := runtime.ConnectionSnapshot(ctx)
			if err != nil {
				continue
			}
			_, connector, _, ok := connectionSnapshotMetadata(snapshot)
			if !ok || connector != connectorID || snapshotConnectionID(snapshot) != connectionID {
				continue
			}
			view, err := runtime.ReadWorkContextView(ctx)
			if err == nil {
				encoded, marshalErr := json.Marshal(view)
				return encoded, viewItemCount(encoded), marshalErr
			}
		}
	} else {
		for _, runtime := range logistics {
			if runtime == nil {
				continue
			}
			snapshot, err := runtime.ConnectionSnapshot(ctx)
			if err != nil {
				continue
			}
			_, connector, _, ok := connectionSnapshotMetadata(snapshot)
			if !ok || connector != connectorID || snapshotConnectionID(snapshot) != connectionID {
				continue
			}
			view, err := runtime.ReadLogisticsView(ctx)
			if err == nil {
				encoded, marshalErr := json.Marshal(view)
				return encoded, viewItemCount(encoded), marshalErr
			}
		}
	}
	return nil, 0, errors.New("view unavailable")
}

func viewItemCount(encoded []byte) uint32 {
	var envelope struct {
		Items []json.RawMessage `json:"items"`
	}
	if json.Unmarshal(encoded, &envelope) != nil || len(envelope.Items) > 128 {
		return 129
	}
	return uint32(len(envelope.Items))
}

func (console *Console) deleteRemoteViewAdmission(id string) {
	console.mu.Lock()
	delete(console.remoteViewAdmissions, id)
	console.mu.Unlock()
}
