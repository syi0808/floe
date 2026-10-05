import 'package:flutter/material.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';

/// Display groups never become selection values or authority references.
final class ConnectionResourceGroups extends StatelessWidget {
  const ConnectionResourceGroups({
    super.key,
    required this.items,
    required this.ungroupedLabel,
    this.columns = false,
  });
  final List<({ResourceGroup? group, Widget child})> items;
  final String ungroupedLabel;
  final bool columns;

  @override
  Widget build(BuildContext context) {
    final groups =
        <ResourceGroupRef?, List<({ResourceGroup? group, Widget child})>>{};
    for (final item in items) {
      (groups[item.group?.groupRef] ??= []).add(item);
    }
    if (!columns && groups.keys.every((key) => key == null)) {
      return Column(
        mainAxisSize: MainAxisSize.min,
        children: [for (final item in items) item.child],
      );
    }
    Widget section(List<({ResourceGroup? group, Widget child})> group) =>
        Column(
          key: ValueKey(group.first.group?.groupRef),
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '${group.first.group?.label ?? ungroupedLabel} · ${group.length}',
              style: FloeType.label.copyWith(
                height: 1.7,
                color: FloePalette.neutral600,
              ),
            ),
            const SizedBox(height: FloeSpace.sm),
            for (final item in group) item.child,
          ],
        );
    if (!columns) {
      return Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final group in groups.values)
            Padding(
              padding: const EdgeInsets.only(bottom: FloeSpace.base),
              child: section(group),
            ),
        ],
      );
    }
    return LayoutBuilder(
      builder: (context, constraints) {
        final count = ((constraints.maxWidth + 24) / 284).floor().clamp(1, 3);
        final width = (constraints.maxWidth - 24 * (count - 1)) / count;
        return Wrap(
          spacing: 24,
          runSpacing: 24,
          children: [
            for (final group in groups.values)
              SizedBox(width: width, child: section(group)),
          ],
        );
      },
    );
  }
}
