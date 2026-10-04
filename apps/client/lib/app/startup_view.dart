import 'package:flutter/material.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/l10n/app_localizations.dart';

/// Presentation only. Host admission has one owner in main; rebuilding this
/// surface never starts another open, retries a key, or resets local data.
class FloeStartupApp extends StatelessWidget {
  const FloeStartupApp.waiting({super.key}) : message = null;
  const FloeStartupApp.failed(String failure, {super.key}) : message = failure;

  final String? message;

  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: FloeTheme.light,
    locale: const Locale('en'),
    supportedLocales: AppLocalizations.supportedLocales,
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: Builder(
      builder: (context) => FloeScaffold(
        body: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 480),
            child: Padding(
              padding: const EdgeInsets.all(32),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (message == null)
                    const CircularProgressIndicator(
                      semanticsLabel: 'Starting Floe',
                    )
                  else
                    const Icon(Icons.error_outline, size: 32),
                  const SizedBox(height: 16),
                  Text(
                    message == null
                        ? 'Starting Floe…'
                        : AppLocalizations.of(context).couldNotStartFloeCore,
                    style: FloeType.title,
                  ),
                  if (message case final failure?) ...[
                    const SizedBox(height: 8),
                    SelectableText(
                      failure,
                      textAlign: TextAlign.center,
                      style: FloeType.body,
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    ),
  );
}
