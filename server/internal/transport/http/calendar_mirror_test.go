package httptransport

import (
	"encoding/json"
	"net/http/httptest"
	"testing"

	"floe/server/internal/trust"
	"floe/server/internal/views"
)

func TestCalendarMirrorWireProjectionKeepsPublicEnvelope(t *testing.T) {
	producer := trust.ProducerMetadata{Audience: "audience", InstanceID: "instance", Fingerprint: "fingerprint"}
	preview := httptest.NewRecorder()
	writeCalendarMirrorPreview(preview, views.CalendarMirrorPreviewResult{SchemaVersion: 1, Descriptor: "descriptor", Signature: "signature", Producer: producer})
	assertJSONKeys(t, preview.Body.Bytes(), []string{"schema_version", "descriptor_b64url", "producer_signature", "producer"})

	challenge := httptest.NewRecorder()
	writeCalendarMirrorChallenge(challenge, views.CalendarMirrorChallengeResult{SchemaVersion: 1, Challenge: "challenge", Signature: "signature", Producer: producer})
	assertJSONKeys(t, challenge.Body.Bytes(), []string{"schema_version", "challenge_b64url", "producer_signature", "producer"})

	release := httptest.NewRecorder()
	writeCalendarMirrorRelease(release, views.CalendarMirrorReleaseResult{SchemaVersion: 1, Page: json.RawMessage(`{"result_kind":"calendar.mirror"}`)})
	var envelope struct {
		SchemaVersion int             `json:"schema_version"`
		Page          json.RawMessage `json:"page"`
	}
	if err := json.Unmarshal(release.Body.Bytes(), &envelope); err != nil || envelope.SchemaVersion != 1 || string(envelope.Page) != `{"result_kind":"calendar.mirror"}` {
		t.Fatalf("release page did not retain raw JSON object envelope: %#v err=%v", envelope, err)
	}
}

func assertJSONKeys(t *testing.T, raw []byte, want []string) {
	t.Helper()
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		t.Fatal(err)
	}
	if len(fields) != len(want) {
		t.Fatalf("JSON fields=%v, want exactly %v", fields, want)
	}
	for _, key := range want {
		if _, ok := fields[key]; !ok {
			t.Errorf("JSON field %q missing from %s", key, raw)
		}
	}
}
