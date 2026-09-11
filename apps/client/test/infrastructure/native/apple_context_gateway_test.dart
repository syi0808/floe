import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';

void main() {
  const observed = 1789056000000;

  test('uses the validated local cache producer ID at the native boundary', () {
    const deviceId = 'local-00000000-0000-4000-8000-000000000001';
    expect(appleNativeArguments(deviceId), {'device_id': deviceId});
    expect(appleNativeArguments(deviceId, {'limit': 64}), {
      'device_id': deviceId,
      'limit': 64,
    });
    expect(
      () => AppleContextGateway(deviceId: 'different device'),
      throwsArgumentError,
    );
  });

  test('accepts bounded Apple native views and capability gate', () {
    validateApplePeopleView({
      'schema_version': 1,
      'view_id': 'people.identity',
      'source_handle': 'people:apple:source',
      'observed_at_unix_ms': observed,
      'expires_at_unix_ms': observed + 300000,
      'coverage_complete': true,
      'identities': [
        {
          'identity_handle': 'person.identity:one',
          'display_name': 'Ada',
          'aliases': ['email:ada@example.test'],
          'confidence_millis': 1000,
          'evidence_handles': ['contact.evidence:one'],
        },
      ],
    });
    validateAppleWellbeingView({
      'schema_version': 1,
      'view_id': 'wellbeing.derived',
      'source_handle': 'wellbeing:apple-health',
      'observed_at_unix_ms': observed,
      'expires_at_unix_ms': observed + 1800000,
      'capacity': 'typical',
      'recovery': 'recovered',
      'confidence_millis': 600,
      'evidence_handles': ['health.sleep.window:one'],
    });
    validateAppleScreenTimeCapability({
      'schema_version': 1,
      'source_handle': 'attention:apple-device-activity',
      'outcome': 'entitlement_unavailable',
      'authorization': 'not_determined',
      'region_availability': 'unknown',
      'observed_at_unix_ms': observed,
    });
  });

  test('accepts feasibility with required WeatherKit attribution', () {
    validateAppleFeasibilityResult({
      'view': {
        'schema_version': 1,
        'view_id': 'schedule.feasibility',
        'source_handle': 'feasibility:apple',
        'observed_at_unix_ms': observed,
        'expires_at_unix_ms': observed + 300000,
        'items': [
          {
            'event_handle': 'event:one',
            'evidence_handles': ['calendar:event:one'],
            'travel_duration_seconds': 900,
            'leave_by_unix_ms': observed + 900000,
            'weather_impact': 'minor',
            'confidence_millis': 800,
          },
        ],
      },
      'weather_attribution': {
        'legal_page_url': 'https://weather.example/legal',
        'combined_mark_light_url': 'https://weather.example/light.svg',
        'combined_mark_dark_url': 'https://weather.example/dark.svg',
      },
    });
  });

  test('rejects raw fields and a falsely supported Screen Time gate', () {
    expect(
      () => validateAppleWellbeingView({
        'schema_version': 1,
        'view_id': 'wellbeing.derived',
        'source_handle': 'wellbeing:apple-health',
        'observed_at_unix_ms': observed,
        'expires_at_unix_ms': observed + 1000,
        'capacity': 'typical',
        'recovery': 'typical',
        'confidence_millis': 600,
        'evidence_handles': ['health.steps.window:one'],
        'steps': 12000,
      }),
      throwsFormatException,
    );
    expect(
      () => validateAppleScreenTimeCapability({
        'schema_version': 1,
        'source_handle': 'attention:apple-device-activity',
        'outcome': 'supported',
        'authorization': 'not_determined',
        'region_availability': 'unknown',
        'observed_at_unix_ms': observed,
        'entitlement_provisioned': true,
      }),
      throwsFormatException,
    );
  });
}
