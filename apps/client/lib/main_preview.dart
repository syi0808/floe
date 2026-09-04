import 'package:flutter/widgets.dart';

import 'app/floe_app.dart';
import 'preview/prototype_fixture.dart';
import 'preview/calendar_fixture.dart';

void main() => runApp(
  prototypeAppearance(
    FloeApp(gateway: calendarPreviewGateway(), query: calendarPreviewQuery),
  ),
);
