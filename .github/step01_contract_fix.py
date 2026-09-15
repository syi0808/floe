#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def edit(path: str, fn):
    target = ROOT / path
    before = target.read_text()
    after = fn(before)
    if before == after:
        raise SystemExit(f"no change: {path}")
    target.write_text(after)


def once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one anchor, found {count}")
    return text.replace(old, new, 1)


# Current internal turn DTO: carry the current app profile preference without a new schema/version.
edit("crates/floe-protocol/src/dto/agent.rs", lambda text: once(
    text,
    "pub struct AgentConversationTurnRequestDto {\n    pub session_id: String,\n",
    "pub struct AgentConversationTurnRequestDto {\n    #[serde(default)]\n    pub profile: super::AppProfileSelectionDto,\n    pub session_id: String,\n",
    "turn DTO profile",
))

# App facade -> existing internal host request: preserve explicit profile and normalized text.
def patch_ffi_lib(text: str) -> str:
    return once(
        text,
        "                    text: request.text,\n                    device_id: caller.device_id().to_owned(),\n",
        "                    text: request.text.trim().to_owned(),\n"
        "                    profile: match request.profile {\n"
        "                        floe_app::ProfileSelection::Auto => AppProfileSelectionDto::Auto,\n"
        "                        floe_app::ProfileSelection::Explicit(profile_id) => {\n"
        "                            AppProfileSelectionDto::Explicit { profile_id }\n"
        "                        }\n"
        "                    },\n"
        "                    device_id: caller.device_id().to_owned(),\n",
        "host profile forwarding",
    )
edit("crates/floe-ffi/src/lib.rs", patch_ffi_lib)

# Existing host root -> Conversation canonical intent: do not substitute Auto.
def patch_turn(text: str) -> str:
    return once(
        text,
        "                    profile: floe_conversation::ProfileSelection::Auto,\n",
        "                    profile: match &request.profile {\n"
        "                        floe_protocol::AppProfileSelectionDto::Auto => {\n"
        "                            floe_conversation::ProfileSelection::Auto\n"
        "                        }\n"
        "                        floe_protocol::AppProfileSelectionDto::Explicit { profile_id } => {\n"
        "                            floe_conversation::ProfileSelection::Explicit(profile_id.clone())\n"
        "                        }\n"
        "                    },\n",
        "conversation root profile",
    )
edit("crates/floe-ffi/src/vault_host/conversation_turn.rs", patch_turn)

# All remaining Rust literals for the internal DTO are current-definition constructors.
# Production app construction above is excluded because it forwards the explicit value.
for target in (ROOT / "crates").rglob("*.rs"):
    rel = target.relative_to(ROOT).as_posix()
    if rel in {
        "crates/floe-protocol/src/dto/agent.rs",
        "crates/floe-ffi/src/lib.rs",
    }:
        continue
    text = target.read_text()
    pattern = re.compile(r"(?m)^([ \t]*)([^\n]*\bAgentConversationTurnRequestDto \{)\n")
    if not pattern.search(text):
        continue
    def add_default(match: re.Match[str]) -> str:
        start = match.end()
        lookahead = text[start:start + 240]
        if re.match(r"[ \t]*profile:\s*", lookahead):
            return match.group(0)
        return match.group(0) + match.group(1) + "    profile: floe_protocol::AppProfileSelectionDto::Auto,\n"
    changed = pattern.sub(add_default, text)
    if changed != text:
        target.write_text(changed)

# Wire and app validation must inspect the same trimmed value as Conversation.
edit("crates/floe-protocol/src/dto/commands.rs", lambda text: once(
    text,
    "                    || text\n                        .chars()",
    "                    || text\n                        .trim()\n                        .chars()",
    "wire normalized controls",
))

def patch_app_services(text: str) -> str:
    text = once(
        text,
        "                .text\n                .chars()",
        "                .text\n                .trim()\n                .chars()",
        "app normalized controls",
    )
    marker = "    #[test]\n    fn cancel_run_rejects_invalid_ids() {"
    test = r'''    #[test]
    fn turn_text_is_checked_after_normalization() {
        for text in ["\thello\t", "\r\nhello\r\n", "\u{85}hello\u{85}", "hello\nworld"] {
            let mut input = request();
            input.text = text.into();
            assert_eq!(input.validate(), Ok(()), "{text:?}");
        }
        for text in [" \t\r\n", "hello\tworld", "hello\rworld", "hello\u{80}world"] {
            let mut input = request();
            input.text = text.into();
            assert_eq!(input.validate(), Err(ServiceError::InvalidInput), "{text:?}");
        }
    }

'''
    return once(text, marker, test + marker, "app text test")
edit("crates/app/src/services.rs", patch_app_services)

# Dart prepares exactly the value that is sent/digested. Match Rust str::trim White_Space.
def patch_dart(text: str) -> str:
    text = once(
        text,
        "    if (_closed) throw StateError('FloeClient is already closed.');\n    if (sessionId.isEmpty ||",
        "    if (_closed) throw StateError('FloeClient is already closed.');\n"
        "    final normalizedText = _trimTurnText(text);\n"
        "    if (sessionId.isEmpty ||",
        "dart normalized text",
    )
    text = once(
        text,
        "        text.trim().isEmpty ||\n        utf8.encode(text.trim()).length > 8 * 1024 ||",
        "        normalizedText.isEmpty ||\n"
        "        utf8.encode(normalizedText).length > 8 * 1024 ||\n"
        "        normalizedText.runes.any(\n"
        "          (value) => value != 0x0a && _isInputControl(value),\n"
        "        ) ||",
        "dart text validation",
    )
    text = once(text, "                profileId.trim() != profileId ||", "                _trimTurnText(profileId) != profileId ||", "dart profile trim")
    text = once(
        text,
        "                profileId.codeUnits.any(\n                  (value) => value < 32 || value == 127,\n                )) ||",
        "                profileId.runes.any(_isInputControl)) ||",
        "dart profile controls",
    )
    text = once(text, "      text: text,\n      continuation: continuation,", "      text: normalizedText,\n      continuation: continuation,", "dart prepared value")
    helpers = r'''

// Match Rust str::trim (Unicode White_Space); BOM is deliberately not trimmed.
bool _isTurnWhitespace(int value) =>
    (value >= 0x09 && value <= 0x0d) ||
    value == 0x20 ||
    value == 0x85 ||
    value == 0xa0 ||
    value == 0x1680 ||
    (value >= 0x2000 && value <= 0x200a) ||
    value == 0x2028 ||
    value == 0x2029 ||
    value == 0x202f ||
    value == 0x205f ||
    value == 0x3000;

String _trimTurnText(String text) {
  var start = 0;
  var end = text.length;
  while (start < end && _isTurnWhitespace(text.codeUnitAt(start))) {
    start++;
  }
  while (end > start && _isTurnWhitespace(text.codeUnitAt(end - 1))) {
    end--;
  }
  return text.substring(start, end);
}

bool _isInputControl(int value) =>
    value < 0x20 || (value >= 0x7f && value <= 0x9f);
'''
    return text.rstrip() + helpers + "\n"
edit("apps/client/lib/runtime_client/floe_client.dart", patch_dart)

# Canonical contract regression: profile must change command identity.
def patch_intent(text: str) -> str:
    marker = "    #[test]\n    fn normalization_uses_utf8_byte_limit() {"
    tests = r'''    #[test]
    fn canonical_profile_selection_is_part_of_the_intent() {
        let mut input = turn("hello");
        let automatic = CanonicalTurnIntent::from_start_turn(&mut input)
            .unwrap().digest("person").unwrap();
        input.profile = ProfileSelection::Explicit("profile-a".into());
        let first = CanonicalTurnIntent::from_start_turn(&mut input)
            .unwrap().digest("person").unwrap();
        input.profile = ProfileSelection::Explicit("profile-b".into());
        let second = CanonicalTurnIntent::from_start_turn(&mut input)
            .unwrap().digest("person").unwrap();
        assert_ne!(automatic, first);
        assert_ne!(first, second);
    }

    #[test]
    fn normalization_preserves_internal_newlines_and_rejects_internal_controls() {
        for text in ["\thello\t", "\r\nhello\r\n", "\u{85}hello\u{85}"] {
            assert_eq!(normalize_turn_text(text).unwrap(), "hello");
        }
        assert_eq!(normalize_turn_text("hello\nworld").unwrap(), "hello\nworld");
        assert_eq!(normalize_turn_text("\u{feff}hello\u{feff}").unwrap(), "\u{feff}hello\u{feff}");
        for text in [" \t\r\n", "hello\tworld", "hello\rworld", "hello\u{80}world"] {
            assert_eq!(normalize_turn_text(text), Err(AgentFailure::InvalidInput));
        }
    }

'''
    return once(text, marker, tests + marker, "intent regressions")
edit("crates/modules/conversation/src/domain/intent.rs", patch_intent)

# App-wire test proves explicit profile reaches the app facade unchanged.
def patch_app_wire(text: str) -> str:
    text = once(
        text,
        "                    profile: AppProfileSelectionDto::Auto,\n                    retry_of: Some(retry_of),",
        "                    profile: AppProfileSelectionDto::Explicit {\n                        profile_id: \"profile-a\".into(),\n                    },\n                    retry_of: Some(retry_of),",
        "app-wire explicit profile",
    )
    return once(
        text,
        "        assert_eq!(captured.retry_of, Some(retry_of));",
        "        assert_eq!(captured.retry_of, Some(retry_of));\n"
        "        assert_eq!(captured.profile, floe_app::ProfileSelection::Explicit(\"profile-a\".into()));",
        "app-wire capture assertion",
    )
edit("crates/floe-ffi/src/app_wire.rs", patch_app_wire)

# Dart regression for the exact prepared/wire value.
def patch_dart_test(text: str) -> str:
    marker = "void main() {\n"
    test = r'''void main() {
  test('StartTurn normalizes input and preserves explicit profile on the wire', () async {
    final transport = FakeTransport();
    final client = FloeClient(transport);
    final command = client.prepareStartTurn(
      sessionId: '00000000-0000-4000-8000-000000000204',
      expectedRevision: 3,
      text: '\t hello\nworld \r\n',
      profileId: 'profile-a',
    );
    expect(command.text, 'hello\nworld');
    transport.command = (request) async => {
      'kind': 'command_receipt',
      'command_id': request['command_id'],
      'runtime_epoch': 7,
      'admission': 'accepted',
      'run_id': '00000000-0000-4000-8000-000000000205',
      'session_revision': 4,
    };
    await client.submitStartTurn(command);
    final wire = transport.commandRequests.single['command'] as Map;
    expect(wire['text'], 'hello\nworld');
    expect(wire['profile'], {'kind': 'explicit', 'profile_id': 'profile-a'});
  });

  test('StartTurn matches Rust whitespace and control handling', () {
    final client = FloeClient(FakeTransport());
    PreparedStartTurn prepare(String text) => client.prepareStartTurn(
      sessionId: '00000000-0000-4000-8000-000000000204',
      expectedRevision: 0,
      text: text,
    );
    for (final text in ['\thello\t', '\r\nhello\r\n', '\u0085hello\u0085']) {
      expect(prepare(text).text, 'hello');
    }
    expect(prepare('hello\nworld').text, 'hello\nworld');
    expect(prepare('\ufeffhello\ufeff').text, '\ufeffhello\ufeff');
    for (final text in [' \t\r\n', 'hello\tworld', 'hello\rworld', 'hello\u0080world']) {
      expect(() => prepare(text), throwsFormatException);
    }
  });
'''
    return once(text, marker, test, "dart regressions")
edit("apps/client/test/runtime_client/floe_client_test.dart", patch_dart_test)

# Existing Worker replay test: explicit profile is part of stored digest and same CommandId conflicts on changes.
path = ROOT / "crates/floe-ffi/src/vault_host/tests/calendar_experts.rs"
text = path.read_text()
start = text.index("fn production_conversation_replays_the_same_request_without_model_redispatch() {")
end = text.index("\n#[test]\n", start)
body = text[start:end]
body = once(body, "            profile: floe_protocol::AppProfileSelectionDto::Auto,", "            profile: floe_protocol::AppProfileSelectionDto::Explicit { profile_id: \"profile-a\".into() },", "replay explicit profile")
body = once(body, '            text: "Answer once".into(),', '            text: "\\t Answer once \\r\\n".into(),', "replay normalized text")
insert = r'''    let stored = worker
        .conversation_query(
            person,
            ConversationQuery::Command(floe_kernel::CommandId::from_uuid(request_id).unwrap()),
        )
        .unwrap()
        .unwrap();
    let mut expected = floe_conversation::StartTurn {
        command_id: floe_kernel::CommandId::from_uuid(request_id).unwrap(),
        session_id: session.id,
        expected_revision: session.revision,
        text: "\t Answer once \r\n".into(),
        mode: floe_conversation::TurnMode::New,
        retry_of: None,
        profile: floe_conversation::ProfileSelection::Explicit("profile-a".into()),
    };
    let expected_digest = floe_conversation::CanonicalTurnIntent::from_start_turn(&mut expected)
        .unwrap().digest(&person.to_string()).unwrap();
    assert_eq!(stored.request_digest, expected_digest);

'''
body = once(body, '    assert_eq!(first.failure, None, "first: {first:?}");\n', '    assert_eq!(first.failure, None, "first: {first:?}");\n' + insert, "stored digest assertion")
conflict = r'''    for profile in [
        floe_protocol::AppProfileSelectionDto::Explicit { profile_id: "profile-b".into() },
        floe_protocol::AppProfileSelectionDto::Auto,
    ] {
        let mut changed_profile = action.clone();
        let AgentVaultActionDto::ConversationTurn { request } = &mut changed_profile else { unreachable!() };
        request.profile = profile;
        worker.request(person, request_id, AgentVaultOperationDto::Submit { action: changed_profile }).unwrap();
        let conflict = wait(&worker, person, request_id);
        worker.request(person, request_id, AgentVaultOperationDto::Release {}).unwrap();
        assert_eq!(conflict.failure, Some(AgentFailure::Conflict));
    }

'''
body = once(body, "    let mut changed = action;\n", conflict + "    let mut changed = action;\n", "profile conflict")
text = text[:start] + body + text[end:]
path.write_text(text)

# Protocol-only contract tests; schema number stays unchanged.
new_test = ROOT / "crates/floe-protocol/tests/turn_input_contract.rs"
if new_test.exists():
    raise SystemExit("turn_input_contract.rs already exists")
new_test.write_text(r'''use floe_protocol::{AppCommandRequestDto, APP_WIRE_VERSION};
use serde_json::json;

fn request(text: &str) -> AppCommandRequestDto {
    serde_json::from_value(json!({
        "schema_version": APP_WIRE_VERSION,
        "request_id": "00000000-0000-4000-8000-000000000101",
        "command_id": "00000000-0000-4000-8000-000000000102",
        "command": {
            "kind": "conversation.start_turn",
            "session_id": "00000000-0000-4000-8000-000000000103",
            "expected_revision": 0,
            "text": text,
            "mode": {"kind": "new_turn"},
            "profile": {"kind": "explicit", "profile_id": "profile-a"}
        }
    })).unwrap()
}

#[test]
fn wire_text_validation_uses_the_normalized_value() {
    for text in ["\thello\t", "\r\nhello\r\n", "\u{85}hello\u{85}", "hello\nworld", "\u{feff}hello\u{feff}"] {
        assert_eq!(request(text).validate(), Ok(()), "{text:?}");
    }
    for text in [" \t\r\n", "hello\tworld", "hello\rworld", "hello\u{80}world"] {
        assert_eq!(request(text).validate(), Err("command.text"), "{text:?}");
    }
    assert_eq!(request(&format!("\t{}\r\n", "a".repeat(8192))).validate(), Ok(()));
    assert_eq!(request(&"a".repeat(8193)).validate(), Err("command.text"));
    assert_eq!(request(&"한".repeat(2731)).validate(), Err("command.text"));
}

#[test]
fn internal_turn_request_roundtrip_preserves_explicit_profile() {
    use floe_protocol::{AgentConversationTurnRequestDto, AppProfileSelectionDto};
    let input = json!({
        "session_id": "00000000-0000-4000-8000-000000000103",
        "expected_revision": 0,
        "text": "hello",
        "device_id": "mac-local",
        "profile": {"kind": "explicit", "profile_id": "profile-a"}
    });
    let decoded: AgentConversationTurnRequestDto = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(decoded.profile, AppProfileSelectionDto::Explicit { profile_id: "profile-a".into() });
    assert_eq!(serde_json::to_value(decoded).unwrap()["profile"], input["profile"]);
    let mut automatic = input;
    automatic.as_object_mut().unwrap().remove("profile");
    let decoded: AgentConversationTurnRequestDto = serde_json::from_value(automatic).unwrap();
    assert_eq!(decoded.profile, AppProfileSelectionDto::Auto);
}
''')

# Verify every Rust struct literal now names profile; no silent incomplete current contract.
missing = []
for target in (ROOT / "crates").rglob("*.rs"):
    if target.as_posix().endswith("crates/floe-protocol/src/dto/agent.rs"):
        continue
    source = target.read_text()
    for match in re.finditer(r"\bAgentConversationTurnRequestDto\s*\{", source):
        close = source.find("}", match.end())
        if close >= 0 and "profile:" not in source[match.end():close]:
            missing.append(str(target.relative_to(ROOT)))
if missing:
    raise SystemExit("constructors missing profile: " + ", ".join(sorted(set(missing))))

print("Step 01 correction applied")
