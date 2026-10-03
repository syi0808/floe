import 'package:flutter/material.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/local_profile_selection.dart';

final class ProfileSelectionApp extends StatefulWidget {
  const ProfileSelectionApp({
    super.key,
    required this.profiles,
    required this.onSelect,
    required this.onReload,
  });
  final List<ExistingLocalProfile> profiles;
  final Future<void> Function(ExistingLocalProfile profile) onSelect;
  final Future<void> Function() onReload;
  @override
  State<ProfileSelectionApp> createState() => _ProfileSelectionAppState();
}

final class _ProfileSelectionAppState extends State<ProfileSelectionApp> {
  ExistingLocalProfile? selected;
  bool busy = false;
  String? failure;
  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Floe',
    theme: FloeTheme.light,
    debugShowCheckedModeBanner: false,
    home: Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 560),
          child: Padding(
            padding: const EdgeInsets.all(32),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  widget.profiles.isEmpty
                      ? 'Profile setup is required'
                      : 'Choose your local profile',
                  style: Theme.of(context).textTheme.headlineSmall,
                ),
                const SizedBox(height: 16),
                Text(
                  widget.profiles.isEmpty
                      ? 'No existing Floe profile was found. Finish profile setup before opening the app.'
                      : 'Select the existing profile you want to open. Your other profiles stay in place.',
                ),
                const SizedBox(height: 16),
                if (widget.profiles.isNotEmpty)
                  SizedBox(
                    height: (widget.profiles.length * 88)
                        .clamp(88, 320)
                        .toDouble(),
                    child: ListView(
                      children: [
                        for (final profile in widget.profiles)
                          RadioListTile<ExistingLocalProfile>(
                            value: profile,
                            groupValue: selected,
                            title: Text(
                              'Profile ${profile.personId.substring(0, 8)}',
                            ),
                            subtitle: Text(profile.personId),
                            onChanged: busy
                                ? null
                                : (value) => setState(() => selected = value),
                          ),
                      ],
                    ),
                  ),
                if (failure != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: Text(failure!),
                  ),
                const SizedBox(height: 16),
                Wrap(
                  spacing: 12,
                  children: [
                    if (widget.profiles.isNotEmpty)
                      FilledButton(
                        onPressed: busy || selected == null
                            ? null
                            : () async {
                                setState(() {
                                  busy = true;
                                  failure = null;
                                });
                                try {
                                  await widget.onSelect(selected!);
                                } on Object {
                                  if (mounted)
                                    setState(
                                      () => failure = 'The selected profile could not be opened.',
                                    );
                                } finally {
                                  if (mounted) setState(() => busy = false);
                                }
                              },
                        child: const Text('Open selected profile'),
                      ),
                    TextButton(
                      onPressed: busy ? null : widget.onReload,
                      child: const Text('Check profiles again'),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}
