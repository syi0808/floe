package codexauth

// Codex subscription protocol reference (not the public paid Live API):
// openai/codex@0df4aa67781ae45a3c97befa4db48b734afe46d9, codex-api
// endpoint/realtime_call.rs and realtime_websocket/*frameless_bidi.rs.
// Provider names and JSON stay in this adapter. No wire/body logging.
import (
	"encoding/json"
	"floe/server/internal/inference"
	"floe/server/internal/trust"
	"net/url"
	"strings"
	"unicode/utf8"
)

const liveFrameLimit = 512 * 1024 // Host frame bound, not model context capacity.
const liveAppendChunkBytes = 500  // Source-verified UTF-8 bytes, not API tokens.

func liveCallID(value string) bool {
	if trust.ValidID(value) {
		return true
	}
	if !strings.HasPrefix(value, "rtc_") || len(value) <= 4 || len(value) > 256 {
		return false
	}
	for _, c := range value[4:] {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_' || c == '-') {
			return false
		}
	}
	return true
}
func parseLiveLocation(value string) (string, error) {
	if len(value) > 2048 {
		return "", invalidOutput
	}
	u, err := url.Parse(value)
	if err != nil || u.User != nil || u.RawQuery != "" || u.Fragment != "" || u.RawPath != "" {
		return "", invalidOutput
	}
	if u.IsAbs() && (u.Scheme != "https" || u.Host != "api.openai.com" && u.Host != "chatgpt.com") {
		return "", invalidOutput
	}
	if !u.IsAbs() && u.Host != "" {
		return "", invalidOutput
	}
	path := u.Path
	if !strings.HasPrefix(path, "/v1/realtime/calls/") && !strings.HasPrefix(path, "/backend-api/codex/realtime/calls/") {
		return "", invalidOutput
	}
	segments := strings.Split(path, "/")
	id := segments[len(segments)-1]
	for _, s := range segments {
		if s == "." || s == ".." {
			return "", invalidOutput
		}
	}
	if !liveCallID(id) {
		return "", invalidOutput
	}
	return id, nil
}

func decodeLiveEvent(raw []byte) (*inference.LiveEvent, error) {
	if trust.StrictJSON(raw, liveFrameLimit, 32) != nil || !inference.ValidJSONTextEncoding(raw) {
		return nil, invalidOutput
	}
	var frame struct {
		Type    string `json:"type"`
		Session struct {
			ID string `json:"id"`
		} `json:"session"`
		Turn struct {
			ID         string `json:"id"`
			Role       string `json:"role"`
			Transcript string `json:"transcript"`
		} `json:"turn"`
		Item struct {
			ID      string `json:"id"`
			Type    string `json:"type"`
			Target  string `json:"target"`
			Text    string `json:"text"`
			Content []struct {
				Type string `json:"type"`
				Text string `json:"text"`
			} `json:"content"`
		} `json:"item"`
	}
	if json.Unmarshal(raw, &frame) != nil || frame.Type == "" {
		return nil, invalidOutput
	}
	var event inference.LiveEvent
	switch frame.Type {
	case "session.started", "session.updated":
		if !inference.LiveIdentifier(frame.Session.ID) {
			return nil, invalidOutput
		}
		event.Kind = "ready"
	case "input_transcript.added", "output_transcript.added":
		if frame.Item.Text == "" {
			return nil, nil
		}
		event = inference.LiveEvent{Kind: "transcript_delta", Speaker: "person", Text: frame.Item.Text}
		if frame.Type == "output_transcript.added" {
			event.Speaker = "assistant"
		}
		// The wire's item.id is not assumed to be the final turn.id.
	case "turn.done":
		if frame.Turn.Role != "user" && frame.Turn.Role != "assistant" {
			return nil, invalidOutput
		}
		event = inference.LiveEvent{Kind: "person_utterance_final", UtteranceID: frame.Turn.ID, Text: frame.Turn.Transcript}
		if frame.Turn.Role == "assistant" {
			event.Kind = "assistant_utterance_final"
		}
	case "delegation.created":
		if frame.Item.Type != "delegation" || frame.Item.Target != "client" {
			return nil, invalidOutput
		}
		event = inference.LiveEvent{Kind: "delegation_requested", DelegationID: frame.Item.ID}
		var hint strings.Builder
		for _, content := range frame.Item.Content {
			if content.Type == "input_text" {
				hint.WriteString(content.Text)
				if hint.Len() > 65536 {
					return nil, invalidOutput
				}
			}
		}
		if hint.Len() > 0 {
			text := hint.String()
			event.InputHint = &text
		}
		// No verified utterance reference exists in this wire shape. Never invent it
		// from delegation.id, active captions or an arbitrary latest transcript.
	case "error":
		return nil, unavailable // Do not return provider bodies/secrets.
	default:
		return nil, nil // Includes raw audio. Never retains it as domain data.
	}
	if event.Validate() != nil {
		return nil, invalidOutput
	}
	return &event, nil
}
func liveAppendFrames(request inference.LiveAppend) ([][]byte, error) {
	if request.Validate() != nil {
		return nil, invalidOutput
	}
	if request.Kind == "instruction" {
		return nil, ErrRequestRejected
	} // No verified Codex instruction-append mapping.
	channel := "commentary"
	if request.Kind == "presentable_result" {
		channel = "speakable"
	}
	var frames [][]byte
	for remaining := request.Text; remaining != ""; {
		n := len(remaining)
		if n > liveAppendChunkBytes {
			n = liveAppendChunkBytes
			for n > 0 && !utf8.RuneStart(remaining[n]) {
				n--
			}
		}
		text := remaining[:n]
		remaining = remaining[n:]
		frame := map[string]any{"type": "session.context.append", "channel": channel, "content": []map[string]string{{"type": "input_text", "text": text}}}
		if request.DelegationID != "" {
			frame["type"] = "delegation.context.append"
			frame["delegation_item_id"] = request.DelegationID
		}
		raw, err := json.Marshal(frame)
		if err != nil || len(raw) > liveFrameLimit {
			return nil, invalidOutput
		}
		frames = append(frames, raw)
	}
	return frames, nil
}
