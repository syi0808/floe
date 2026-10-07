import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/service_presentation.dart';
import 'package:floe_client/l10n/app_localizations_en.dart';

Map<String, Object?> _integrationJson({
  String serviceKind = 'synthetic_qa_calendar',
}) => {
  'integration_ref': '1ddbd2e1-42e2-4a5c-9201-58dc8f094f87',
  'revision': 1,
  'service_kind': serviceKind,
  'display_name': 'Synthetic QA Calendar',
  'category': 'calendar',
  'state': 'available',
  'capabilities': <String>[],
};

void main() {
  test('strict integration parser accepts the synthetic wire identity', () {
    final integration = IntegrationSummary.fromJson(_integrationJson());

    expect(integration.serviceKind, 'synthetic_qa_calendar');
    expect(integration.displayName, 'Synthetic QA Calendar');
    expect(
      () => IntegrationSummary.fromJson(
        _integrationJson(serviceKind: 'synthetic_calendar'),
      ),
      throwsFormatException,
    );
  });

  test('synthetic service presentation stays visibly synthetic', () {
    final integration = IntegrationSummary.fromJson(_integrationJson());
    final presentation = ServicePresentation.forIntegration(
      integration,
      AppLocalizationsEn(),
    );

    expect(presentation.name, 'Synthetic QA Calendar');
    expect(presentation.description, contains('synthetic'));
    expect(presentation.description, contains('Linux QA'));
    expect(presentation.description, contains('No real calendar'));
  });
}
