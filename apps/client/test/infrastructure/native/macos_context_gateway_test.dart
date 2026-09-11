import 'package:floe_client/infrastructure/native/macos_context_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('macOS Attention projection accepts a bounded coarse View', () {
    final view = <String, dynamic>{
      'schema_version': 1,
      'view_id': 'attention.coarse',
      'source_handle': 'attention:macos_local',
      'observed_at_unix_ms': 1000,
      'expires_at_unix_ms': 61000,
      'state': 'focused',
      'confidence_millis': 750,
      'evidence_handles': <String>[
        'attention.macos:stable_activity',
        'attention.macos:recent_input',
      ],
    };
    expect(() => validateMacOSAttentionView(view), returnsNormally);

    final leaked = Map<String, dynamic>.from(view)
      ..['bundle_id'] = 'com.example.private';
    expect(() => validateMacOSAttentionView(leaked), throwsFormatException);
  });

  test('macOS Attention projection enforces unknown semantics', () {
    final unknown = <String, dynamic>{
      'schema_version': 1,
      'view_id': 'attention.coarse',
      'source_handle': 'attention:macos_local',
      'observed_at_unix_ms': 1000,
      'expires_at_unix_ms': 61000,
      'state': 'unknown',
      'confidence_millis': 0,
      'evidence_handles': <String>[],
    };
    expect(() => validateMacOSAttentionView(unknown), returnsNormally);

    final inventedEvidence = Map<String, dynamic>.from(unknown)
      ..['confidence_millis'] = 500;
    expect(
      () => validateMacOSAttentionView(inventedEvidence),
      throwsFormatException,
    );
  });
}
