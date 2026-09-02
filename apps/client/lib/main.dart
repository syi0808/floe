import 'package:flutter/material.dart';

import 'app/floe_app.dart';
import 'app/floe_theme.dart';
import 'features/day_canvas/application/ffi_day_gateway.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  try {
    final gateway = await FfiDayGateway.openDefault();
    runApp(FloeApp(gateway: gateway));
  } on Object catch (error) {
    runApp(_StartupErrorApp(message: error.toString()));
  }
}

class _StartupErrorApp extends StatelessWidget {
  const _StartupErrorApp({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: FloeTheme.light,
    home: Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: Padding(
            padding: const EdgeInsets.all(32),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(Icons.error_outline, size: 32),
                const SizedBox(height: 16),
                const Text('Floe Core를 시작하지 못했어요'),
                const SizedBox(height: 8),
                SelectableText(message, textAlign: TextAlign.center),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}
