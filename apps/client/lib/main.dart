import 'package:floe_client/l10n/app_localizations.dart';
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
    locale: const Locale('en'),
    supportedLocales: AppLocalizations.supportedLocales,
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: Builder(
      builder: (context) => Scaffold(
        body: Center(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: 480),
            child: Padding(
              padding: EdgeInsets.all(32),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.error_outline, size: 32),
                  SizedBox(height: 16),
                  Text(AppLocalizations.of(context).couldNotStartFloeCore),
                  SizedBox(height: 8),
                  SelectableText(message, textAlign: TextAlign.center),
                ],
              ),
            ),
          ),
        ),
      ),
    ),
  );
}
