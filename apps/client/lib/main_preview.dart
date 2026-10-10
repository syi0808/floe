import 'package:flutter/widgets.dart';

import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/preview/design_feedback_overlay.dart';
import 'package:floe_client/preview/product_fixture.dart';
import 'package:floe_client/preview/calendar_fixture.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';

void main() => runApp(
  previewAppearance(
    FloeApp(
      personId: calendarPreviewQuery.personId,
      gateway: calendarPreviewGateway(),
      query: calendarPreviewQuery,
      ownerGateways: const LocalOwnerGateways(),
      builder: (context, child) => DesignFeedbackOverlay(child: child!),
    ),
  ),
);
