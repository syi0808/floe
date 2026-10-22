import 'package:flutter/widgets.dart';
import 'package:floe_client/preview/native_interaction_fixture.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final fixture = await NativeInteractionPreviewFixture.create();
  runApp(fixture.buildWidget());
}
