import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../l10n/app_localizations.dart';
import 'design_tokens.dart';
import 'floe_motion.dart';

enum FloeToastTone { success, info }

class FloeToastHost extends StatefulWidget {
  const FloeToastHost({super.key, required this.child});

  final Widget child;

  static FloeToastHostState of(BuildContext context) =>
      context.findAncestorStateOfType<FloeToastHostState>()!;

  @override
  State<FloeToastHost> createState() => FloeToastHostState();
}

class FloeToastHostState extends State<FloeToastHost>
    with WidgetsBindingObserver {
  final _entries = <_ToastEntry>[];
  Timer? _clock;
  Timer? _hoverExit;
  bool _hovered = false;
  bool _focused = false;
  bool _active = true;
  int _nextId = 0;

  void show({
    required String title,
    String? description,
    FloeToastTone tone = FloeToastTone.success,
    String? actionLabel,
    VoidCallback? onAction,
  }) {
    assert((actionLabel == null) == (onAction == null));
    setState(() {
      if (_entries.length == 3) _entries.removeAt(0);
      _entries.add(
        _ToastEntry(
          id: _nextId++,
          title: title,
          description: description,
          tone: tone,
          actionLabel: actionLabel,
          onAction: onAction,
        ),
      );
    });
    _clock ??= Timer.periodic(
      const Duration(milliseconds: 100),
      (_) => _tick(),
    );
  }

  void _tick() {
    final accessible =
        MediaQuery.maybeOf(context)?.accessibleNavigation ?? false;
    final removed = <_ToastEntry>[];
    for (final entry in _entries) {
      if (entry.leaving) {
        entry.exitRemaining -= 100;
        if (entry.exitRemaining <= 0) removed.add(entry);
      } else if (!_hovered &&
          !_focused &&
          _active &&
          !(accessible && entry.onAction != null)) {
        entry.remaining -= 100;
        if (entry.remaining <= 0) _dismiss(entry);
      }
    }
    if (removed.isNotEmpty) {
      setState(() => _entries.removeWhere(removed.contains));
    }
    if (_entries.isEmpty) {
      _clock?.cancel();
      _clock = null;
      _hovered = false;
      _focused = false;
    }
  }

  void _dismiss(_ToastEntry entry) {
    if (entry.leaving) return;
    setState(() {
      entry.leaving = true;
      entry.exitRemaining = FloeMotion.reduceMotion(context) ? 0 : 200;
    });
  }

  void _hover(bool value) {
    _hoverExit?.cancel();
    if (value) {
      if (!_hovered) setState(() => _hovered = true);
    } else {
      _hoverExit = Timer(const Duration(milliseconds: 150), () {
        if (mounted) setState(() => _hovered = false);
      });
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _active =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _active = state == AppLifecycleState.resumed;
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _clock?.cancel();
    _hoverExit?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final media = MediaQuery.of(context);
    final narrow = media.size.width <= 780;
    final duration = FloeMotion.reduceMotion(context)
        ? Duration.zero
        : FloeMotion.notificationDuration;
    return Stack(
      children: [
        Positioned.fill(child: widget.child),
        if (_entries.isNotEmpty)
          Positioned(
            right: math.max(narrow ? 16 : 24, media.padding.right),
            bottom: media.viewInsets.bottom > 0
                ? media.viewInsets.bottom + 16
                : media.padding.bottom + (narrow ? 96 : 24),
            width: narrow
                ? math.max(
                    0,
                    media.size.width -
                        math.max(16, media.padding.left) -
                        math.max(16, media.padding.right),
                  )
                : 356,
            child: Focus(
              onFocusChange: (value) => setState(() => _focused = value),
              onKeyEvent: (_, event) {
                if (event is KeyDownEvent &&
                    event.logicalKey == LogicalKeyboardKey.escape) {
                  _dismiss(_entries.last);
                  return KeyEventResult.handled;
                }
                return KeyEventResult.ignored;
              },
              child: TweenAnimationBuilder<double>(
                tween: Tween(end: _hovered || _focused ? 1 : 0),
                duration: duration,
                curve: FloeMotion.easeOut,
                builder: (context, expansion, _) => MouseRegion(
                  opaque: false,
                  onEnter: (_) => _hover(true),
                  onExit: (_) => _hover(false),
                  child: _ToastStack(
                    expansion: expansion,
                    children: [
                      for (var index = 0; index < _entries.length; index++)
                        Transform.scale(
                          key: ValueKey(_entries[index].id),
                          scale:
                              1 -
                              (_entries.length - 1 - index) *
                                  0.045 *
                                  (1 - expansion),
                          alignment: Alignment.bottomCenter,
                          child: _ToastCard(
                            entry: _entries[index],
                            contentOpacity: index == _entries.length - 1
                                ? 1
                                : const Interval(0.65, 1).transform(expansion),
                            dismiss: () => _dismiss(_entries[index]),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

class _ToastEntry {
  _ToastEntry({
    required this.id,
    required this.title,
    required this.description,
    required this.tone,
    required this.actionLabel,
    required this.onAction,
  });
  final int id;
  final String title;
  final String? description;
  final FloeToastTone tone;
  final String? actionLabel;
  final VoidCallback? onAction;
  int remaining = 4500;
  int exitRemaining = 200;
  bool leaving = false;
}

class _ToastStack extends MultiChildRenderObjectWidget {
  const _ToastStack({required this.expansion, required super.children});

  final double expansion;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderToastStack(expansion);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderToastStack renderObject,
  ) {
    renderObject.expansion = expansion;
  }
}

class _ToastParentData extends ContainerBoxParentData<RenderBox> {}

class _RenderToastStack extends RenderBox
    with
        ContainerRenderObjectMixin<RenderBox, _ToastParentData>,
        RenderBoxContainerDefaultsMixin<RenderBox, _ToastParentData> {
  _RenderToastStack(this._expansion);
  double _expansion;
  Rect _hitBounds = Rect.zero;

  set expansion(double value) {
    if (_expansion == value) return;
    _expansion = value;
    markNeedsLayout();
  }

  @override
  void setupParentData(RenderBox child) {
    if (child.parentData is! _ToastParentData) {
      child.parentData = _ToastParentData();
    }
  }

  @override
  void performLayout() {
    final children = getChildrenAsList();
    final width = constraints.maxWidth;
    final naturalHeights = <double>[];
    final naturalConstraints = BoxConstraints(
      minWidth: width,
      maxWidth: width,
      maxHeight: math.max(
        0,
        (constraints.maxHeight - (children.length - 1) * 10) / children.length,
      ),
    );
    for (final child in children) {
      child.layout(naturalConstraints, parentUsesSize: true);
      naturalHeights.add(child.size.height);
    }
    final frontHeight = naturalHeights.last;
    final collapsedHeight = frontHeight + (children.length - 1) * 9;
    final expandedHeight =
        naturalHeights.fold<double>(0, (sum, height) => sum + height) +
        (children.length - 1) * 10;
    size = constraints.constrain(
      Size(
        width,
        collapsedHeight + (expandedHeight - collapsedHeight) * _expansion,
      ),
    );
    var offset = 0.0;
    var top = size.height;
    for (var index = children.length - 1; index >= 0; index--) {
      final child = children[index];
      final depth = children.length - 1 - index;
      final height =
          frontHeight + (naturalHeights[index] - frontHeight) * _expansion;
      child.layout(
        BoxConstraints.tight(Size(size.width, height)),
        parentUsesSize: true,
      );
      final bottom = depth * 9 + (offset - depth * 9) * _expansion;
      (child.parentData! as _ToastParentData).offset = Offset(
        0,
        size.height - height - bottom,
      );
      top = math.min(top, size.height - height - bottom);
      offset += naturalHeights[index] + 10;
    }
    _hitBounds = Rect.fromLTRB(0, top, size.width, size.height);
  }

  @override
  void paint(PaintingContext context, Offset offset) =>
      defaultPaint(context, offset);

  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) =>
      defaultHitTestChildren(result, position: position);

  @override
  bool hitTestSelf(Offset position) => _hitBounds.contains(position);
}

class _ToastCard extends StatelessWidget {
  const _ToastCard({
    required this.entry,
    required this.contentOpacity,
    required this.dismiss,
  });
  final _ToastEntry entry;
  final double contentOpacity;
  final VoidCallback dismiss;

  @override
  Widget build(BuildContext context) {
    final success = entry.tone == FloeToastTone.success;
    final reduced = FloeMotion.reduceMotion(context);
    return IgnorePointer(
      ignoring: entry.leaving,
      child: TweenAnimationBuilder<double>(
        tween: Tween(begin: 0, end: entry.leaving ? 0 : 1),
        duration: reduced ? Duration.zero : FloeMotion.selectionDuration,
        curve: FloeMotion.easeOut,
        builder: (context, progress, child) => Opacity(
          opacity: progress,
          child: Transform.translate(
            offset: Offset(0, 12 * (1 - progress)),
            child: child,
          ),
        ),
        child: Material(
          key: ValueKey('toast-card-${entry.id}'),
          clipBehavior: Clip.antiAlias,
          color: FloePalette.neutral0,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(16),
            side: BorderSide(color: FloePalette.neutral200),
          ),
          elevation: 6,
          shadowColor: FloePalette.neutral950.withValues(alpha: 0.16),
          child: SingleChildScrollView(
            padding: EdgeInsets.fromLTRB(17, 17, 10, 17),
            child: Opacity(
              opacity: contentOpacity,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  Container(
                    width: 28,
                    height: 28,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: success
                          ? FloePalette.mint50
                          : FloePalette.primary50,
                    ),
                    child: Icon(
                      success ? LucideIcons.check : LucideIcons.info,
                      size: 17,
                      color: success
                          ? FloePalette.mint700
                          : FloePalette.primary600,
                    ),
                  ),
                  SizedBox(width: FloeSpace.md),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Semantics(
                          liveRegion: true,
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                entry.title,
                                style: FloeType.controlLabel.copyWith(
                                  height: 1.6,
                                  color: FloePalette.neutral950,
                                ),
                              ),
                              if (entry.description != null) ...[
                                SizedBox(height: 3),
                                Text(
                                  entry.description!,
                                  style: FloeType.bodySmall.copyWith(
                                    fontSize: 12,
                                    color: FloePalette.neutral600,
                                  ),
                                ),
                              ],
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                  if (entry.onAction != null) ...[
                    SizedBox(width: FloeSpace.md),
                    TextButton(
                      style: TextButton.styleFrom(
                        backgroundColor: FloePalette.primary50,
                        foregroundColor: FloePalette.primary700,
                        padding: EdgeInsets.symmetric(
                          horizontal: 10,
                          vertical: 6,
                        ),
                        minimumSize: Size(0, 32),
                        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(8),
                        ),
                        textStyle: FloeType.label,
                      ),
                      onPressed: () {
                        if (entry.leaving) return;
                        dismiss();
                        entry.onAction!();
                      },
                      child: Text(entry.actionLabel!),
                    ),
                    SizedBox(width: FloeSpace.xs),
                  ],
                  IconButton(
                    tooltip: AppLocalizations.of(context).close,
                    onPressed: dismiss,
                    constraints: BoxConstraints(minWidth: 32, minHeight: 32),
                    padding: EdgeInsets.zero,
                    icon: Icon(
                      LucideIcons.x,
                      size: 14,
                      color: FloePalette.neutral500,
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
}
