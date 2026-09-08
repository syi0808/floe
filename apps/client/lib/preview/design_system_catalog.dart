import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../app/design_tokens.dart';
import '../app/floe_action_card.dart';
import '../app/floe_badge.dart';
import '../app/floe_button.dart';
import '../app/floe_input.dart';
import '../app/floe_primitives.dart';
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
  Widget build(BuildContext context) => FloeScaffold(
    body: SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(FloeSpace.xl),
        child: Align(
          alignment: Alignment.topCenter,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 960),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text('Floe design system', style: FloeType.display),
                const SizedBox(height: FloeSpace.lg),
                const _CatalogSection(
                  title: 'Typography',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Display', style: FloeType.display),
                      Text('Headline', style: FloeType.headline),
                      Text('Title', style: FloeType.title),
                      Text(
                        'Body text for comfortable reading.',
                        style: FloeType.body,
                      ),
                      Text('Control label', style: FloeType.controlLabel),
                      Text('Supporting caption', style: FloeType.caption),
                    ],
                  ),
                ),
                const SizedBox(height: FloeSpace.lg),
                const _CatalogSection(
                  title: 'Status badges',
                  child: Wrap(
                    spacing: FloeSpace.sm,
                    runSpacing: FloeSpace.sm,
                    children: [
                      FloeBadge(label: 'Neutral'),
                      FloeBadge(
                        label: 'Connected',
                        tone: FloeBadgeTone.success,
                      ),
                      FloeBadge(label: 'In progress', tone: FloeBadgeTone.info),
                      FloeBadge(
                        label: 'Needs review',
                        tone: FloeBadgeTone.warning,
                      ),
                      FloeBadge(label: 'Failed', tone: FloeBadgeTone.danger),
                    ],
                  ),
                ),
                const SizedBox(height: FloeSpace.lg),
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
                          onChanged: (value) =>
                              setState(() => calendar = value),
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
                      FloeActionCard(
                        leading: const Icon(Icons.auto_awesome_outlined),
                        title: const Text('Floe is here to help'),
                        description: const Text('Start a conversation'),
                        trailing: const Icon(Icons.arrow_forward),
                        onPressed: () {},
                      ),
                    ],
                  ),
                ),
              ],
            ),
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
