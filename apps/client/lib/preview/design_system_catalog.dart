import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../app/design_tokens.dart';
import '../app/floe_button.dart';
import '../app/floe_input.dart';
import '../app/floe_selection.dart';
import '../app/floe_squircle.dart';
import '../app/floe_switch.dart';
import '../app/floe_time_picker.dart';

class DesignSystemCatalog extends StatefulWidget {
  const DesignSystemCatalog({super.key});

  @override
  State<DesignSystemCatalog> createState() => _DesignSystemCatalogState();
}

class _DesignSystemCatalogState extends State<DesignSystemCatalog> {
  String? calendar = 'personal';
  bool checked = true;
  bool loading = false;
  TimeOfDay time = const TimeOfDay(hour: 9, minute: 30);

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Floe design system')),
    body: SingleChildScrollView(
      padding: const EdgeInsets.all(FloeSpace.xl),
      child: Align(
        alignment: Alignment.topCenter,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 960),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const _CatalogSection(
                title: 'Interaction colors',
                child: Wrap(
                  spacing: FloeSpace.md,
                  runSpacing: FloeSpace.md,
                  children: [
                    _ColorSample('Surface', FloeColor.surface),
                    _ColorSample('Neutral hover', FloeColor.neutralHover),
                    _ColorSample('Quiet hover', FloeColor.quietHover),
                    _ColorSample('Selection hover', FloeColor.selectionHover),
                    _ColorSample('Pressed', FloeColor.neutralPressed),
                    _ColorSample('Focus', FloeColor.focus),
                  ],
                ),
              ),
              const SizedBox(height: FloeSpace.lg),
              _CatalogSection(
                title: 'Buttons',
                child: Wrap(
                  spacing: FloeSpace.md,
                  runSpacing: FloeSpace.md,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    FloeButton.filled(
                      loading: loading,
                      onPressed: () => setState(() => loading = !loading),
                      child: const Text('Primary'),
                    ),
                    FloeButton.outlined(
                      onPressed: () {},
                      child: const Text('Secondary'),
                    ),
                    FloeButton.text(
                      onPressed: () {},
                      child: const Text('Tertiary'),
                    ),
                    const FloeButton.filled(
                      onPressed: null,
                      child: Text('Disabled'),
                    ),
                    FloeButton.icon(
                      tooltip: 'Standard icon',
                      onPressed: () {},
                      icon: const Icon(LucideIcons.settings),
                    ),
                    FloeButton.icon(
                      size: FloeButtonSize.compact,
                      tooltip: 'Compact icon',
                      onPressed: () {},
                      icon: const Icon(LucideIcons.ellipsis),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: FloeSpace.lg),
              _CatalogSection(
                title: 'Fields',
                child: Wrap(
                  spacing: FloeSpace.base,
                  runSpacing: FloeSpace.base,
                  children: [
                    const SizedBox(
                      width: 280,
                      child: FloeInput(
                        label: 'Event title',
                        placeholder: 'Add a title',
                      ),
                    ),
                    const SizedBox(
                      width: 280,
                      child: FloeInput(
                        label: 'Invalid field',
                        errorText: 'Review this value',
                      ),
                    ),
                    SizedBox(
                      width: 280,
                      child: FloeSelect<String>(
                        label: 'Calendar',
                        value: calendar,
                        options: const [
                          FloeSelectOption(
                            value: 'personal',
                            label: 'Personal',
                          ),
                          FloeSelectOption(value: 'work', label: 'Work'),
                        ],
                        onChanged: (value) => setState(() => calendar = value),
                      ),
                    ),
                    FloeSquircle(
                      fill: FloeColor.surfaceSubtle,
                      child: FloeTimePickerButton(
                        value: time,
                        semanticLabel: 'Event time',
                        onChanged: (value) => setState(() => time = value),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: FloeSpace.lg),
              _CatalogSection(
                title: 'Selection',
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    FloeCheckboxTile(
                      value: checked,
                      title: const Text('Include personal calendar'),
                      onChanged: (value) => setState(() => checked = value!),
                    ),
                    const FloeCheckboxTile(
                      value: false,
                      title: Text('Unavailable option'),
                      onChanged: null,
                    ),
                    FloeSwitch(
                      value: checked,
                      label: const Text('Use connected calendar context'),
                      onChanged: (value) => setState(() => checked = value),
                    ),
                    const FloeSwitch(
                      value: false,
                      label: Text('Unavailable integration'),
                      onChanged: null,
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}

class _CatalogSection extends StatelessWidget {
  const _CatalogSection({required this.title, required this.child});

  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) => FloeCard(
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: FloeType.headline),
        const SizedBox(height: FloeSpace.base),
        child,
      ],
    ),
  );
}

class _ColorSample extends StatelessWidget {
  const _ColorSample(this.label, this.color);

  final String label;
  final Color color;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: 132,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        FloeSquircle(
          size: FloeSquircleSize.sm,
          fill: color,
          borderColor: FloeColor.border,
          child: const SizedBox(height: 52),
        ),
        const SizedBox(height: FloeSpace.sm),
        Text(label, style: FloeType.label),
      ],
    ),
  );
}
