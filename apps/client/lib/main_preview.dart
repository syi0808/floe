import 'package:flutter/widgets.dart';

import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/preview/design_feedback_overlay.dart';
import 'package:floe_client/preview/product_fixture.dart';
import 'package:floe_client/preview/calendar_fixture.dart';

void main() => runApp(
  previewAppearance(
    FloeApp(
      gateway: calendarPreviewGateway(),
      query: calendarPreviewQuery,
      builder: (context, child) => DesignFeedbackOverlay(child: child!),
    ),
  ),
);
