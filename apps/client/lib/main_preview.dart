import 'package:flutter/widgets.dart';

import 'app/floe_app.dart';
import 'preview/design_feedback_overlay.dart';
import 'preview/product_fixture.dart';
import 'preview/calendar_fixture.dart';

void main() => runApp(
  previewAppearance(
    FloeApp(
      gateway: calendarPreviewGateway(),
      query: calendarPreviewQuery,
      builder: (context, child) => DesignFeedbackOverlay(child: child!),
    ),
  ),
);
