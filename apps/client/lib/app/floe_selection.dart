import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';
import 'floe_squircle.dart';

@immutable
class FloeSelectOption<T> {
  const FloeSelectOption({
    required this.value,
    required this.label,
    this.description,
    this.enabled = true,
  });

  final T value;
  final String label;
  final String? description;
  final bool enabled;
}

class FloeSelect<T> extends StatelessWidget {
  const FloeSelect({
    required this.label,
    required this.value,
    required this.options,
    required this.onChanged,
    this.enabled = true,
    this.placeholder = 'Choose an option',
    this.description,
    this.validator,
    this.autovalidateMode,
    super.key,
  });

  final String label;
  final T? value;
  final List<FloeSelectOption<T>> options;
  final ValueChanged<T?> onChanged;
  final bool enabled;
  final String placeholder;
  final String? description;
  final FormFieldValidator<T>? validator;
  final AutovalidateMode? autovalidateMode;

  @override
  Widget build(BuildContext context) => FormField<T>(
    key: ValueKey(value),
    initialValue: value,
    enabled: enabled,
    validator: validator,
    autovalidateMode: autovalidateMode,
    builder: (field) => _FloeSelectionAnchor<T>(
      label: label,
      value: field.value,
      options: options,
      enabled: enabled,
      placeholder: placeholder,
      description: description,
      errorText: field.errorText,
      onSelected: (nextValue) {
        field.didChange(nextValue);
        onChanged(nextValue);
      },
    ),
  );
}

class FloeDropdown<T> extends StatelessWidget {
  const FloeDropdown({
    required this.label,
    required this.items,
    required this.onSelected,
    this.enabled = true,
    this.icon,
    super.key,
  });

  final String label;
  final List<FloeSelectOption<T>> items;
  final ValueChanged<T> onSelected;
  final bool enabled;
  final Widget? icon;

  @override
  Widget build(BuildContext context) => _FloeSelectionAnchor<T>(
    label: label,
    options: items,
    enabled: enabled,
    menu: true,
    icon: icon,
    onSelected: onSelected,
  );
}

class _FloeSelectionAnchor<T> extends StatefulWidget {
  const _FloeSelectionAnchor({
    required this.label,
    required this.options,
    required this.enabled,
    required this.onSelected,
    this.value,
    this.placeholder,
    this.description,
    this.errorText,
    this.menu = false,
    this.icon,
  });

  final String label;
  final T? value;
  final List<FloeSelectOption<T>> options;
  final bool enabled;
  final ValueChanged<T> onSelected;
  final String? placeholder;
  final String? description;
  final String? errorText;
  final bool menu;
  final Widget? icon;

  @override
  State<_FloeSelectionAnchor<T>> createState() =>
      _FloeSelectionAnchorState<T>();
}

class _FloeSelectionAnchorState<T> extends State<_FloeSelectionAnchor<T>>
    with SingleTickerProviderStateMixin {
  final triggerKey = GlobalKey();
  final focusNode = FocusNode();
  final popupFocusNode = FocusNode();
  final optionKeys = <GlobalKey>[];
  late final AnimationController popupAnimation;
  OverlayEntry? popupEntry;
  bool open = false;
  bool opensAbove = false;
  int active = -1;
  String searchText = '';
  DateTime? searchedAt;

  FloeSelectOption<T>? get selected {
    for (final option in widget.options) {
      if (option.value == widget.value) return option;
    }
    return null;
  }

  int get firstEnabled => _nextEnabled(-1, 1);

  @override
  void initState() {
    super.initState();
    popupAnimation = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 160),
    );
  }

  @override
  void didUpdateWidget(_FloeSelectionAnchor<T> oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.enabled && open) close(restoreFocus: false);
  }

  @override
  void dispose() {
    final entry = popupEntry;
    popupEntry = null;
    entry?.remove();
    popupAnimation.dispose();
    focusNode.dispose();
    popupFocusNode.dispose();
    super.dispose();
  }

  void toggle() {
    if (!widget.enabled) return;
    open ? close() : show();
  }

  void show({bool last = false}) {
    if (open || popupEntry != null || !widget.enabled) return;
    final overlay = Overlay.of(context);
    final overlayBox = overlay.context.findRenderObject()! as RenderBox;
    final triggerBox =
        triggerKey.currentContext!.findRenderObject()! as RenderBox;
    final anchorOffset = triggerBox.localToGlobal(
      Offset.zero,
      ancestor: overlayBox,
    );
    final anchor = anchorOffset & triggerBox.size;
    final availableWidth = overlayBox.size.width - 24;
    final menuWidth = math.min(math.max(anchor.width, 240.0), availableWidth);
    final naturalHeight = widget.options.isEmpty
        ? 54.0
        : widget.options.fold<double>(
            10,
            (height, option) => height + (option.description == null ? 44 : 59),
          );
    final below = overlayBox.size.height - anchor.bottom - 16;
    final above = anchor.top - 16;
    opensAbove = below < math.min(320, naturalHeight) && above > below;
    final maxHeight = math.max(
      44.0,
      math.min(320.0, opensAbove ? above : below),
    );
    final popupHeight = math.min(naturalHeight, maxHeight);
    final left = math.max(
      12.0,
      math.min(anchor.left, overlayBox.size.width - menuWidth - 12),
    );
    final top = opensAbove
        ? math.max(12.0, anchor.top - popupHeight - 6)
        : anchor.bottom + 6;
    final selectedIndex = widget.options.indexWhere(
      (option) => option.enabled && option.value == widget.value,
    );
    active = !widget.menu && selectedIndex >= 0
        ? selectedIndex
        : last
        ? _nextEnabled(0, -1)
        : firstEnabled;
    optionKeys
      ..clear()
      ..addAll(List.generate(widget.options.length, (_) => GlobalKey()));
    searchText = '';
    searchedAt = null;
    setState(() => open = true);
    popupEntry = OverlayEntry(
      builder: (context) => _buildOverlay(
        left: left,
        top: top,
        width: menuWidth,
        maxHeight: maxHeight,
      ),
    );
    overlay.insert(popupEntry!);
    popupAnimation.duration = FloeMotion.reduceMotion(context)
        ? Duration.zero
        : const Duration(milliseconds: 160);
    popupAnimation.forward(from: 0);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && open) popupFocusNode.requestFocus();
    });
  }

  void close({bool restoreFocus = true, VoidCallback? onClosed}) {
    final entry = popupEntry;
    if (!open || entry == null) return;
    searchText = '';
    searchedAt = null;
    if (mounted) setState(() => open = false);
    if (restoreFocus) focusNode.requestFocus();
    popupAnimation.reverse().whenCompleteOrCancel(() {
      if (popupEntry != entry) return;
      popupEntry = null;
      entry.remove();
      popupAnimation.reset();
      if (mounted) onClosed?.call();
    });
  }

  void choose(int index) {
    if (index < 0 ||
        index >= widget.options.length ||
        !widget.options[index].enabled) {
      return;
    }
    final value = widget.options[index].value;
    final onSelected = widget.onSelected;
    close(onClosed: () => onSelected(value));
  }

  int _nextEnabled(int current, int direction) {
    for (var offset = 1; offset <= widget.options.length; offset++) {
      final index =
          (current + direction * offset + widget.options.length) %
          widget.options.length;
      if (widget.options[index].enabled) return index;
    }
    return -1;
  }

  void setActive(int index, {bool reveal = false}) {
    if (index == active || index < 0 || !widget.options[index].enabled) return;
    active = index;
    popupEntry?.markNeedsBuild();
    if (!reveal) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final optionContext = optionKeys[index].currentContext;
      if (optionContext != null) {
        Scrollable.ensureVisible(
          optionContext,
          alignment: .5,
          duration: FloeMotion.reduceMotion(context)
              ? Duration.zero
              : const Duration(milliseconds: 80),
        );
      }
    });
  }

  KeyEventResult navigate(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.arrowDown) {
      setActive(_nextEnabled(active, 1), reveal: true);
    } else if (key == LogicalKeyboardKey.arrowUp) {
      setActive(_nextEnabled(active < 0 ? 0 : active, -1), reveal: true);
    } else if (key == LogicalKeyboardKey.home) {
      setActive(firstEnabled, reveal: true);
    } else if (key == LogicalKeyboardKey.end) {
      setActive(_nextEnabled(0, -1), reveal: true);
    } else if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.space) {
      choose(active);
    } else if (key == LogicalKeyboardKey.escape) {
      close();
    } else if (key == LogicalKeyboardKey.tab) {
      close(restoreFocus: false);
      return KeyEventResult.ignored;
    } else if (event.character?.length == 1 &&
        !HardwareKeyboard.instance.isMetaPressed &&
        !HardwareKeyboard.instance.isControlPressed &&
        !HardwareKeyboard.instance.isAltPressed) {
      final now = DateTime.now();
      final character = event.character!.toLowerCase();
      searchText =
          searchedAt != null &&
              now.difference(searchedAt!) < const Duration(milliseconds: 700)
          ? '$searchText$character'
          : character;
      searchedAt = now;
      final repeated = searchText
          .split('')
          .every((candidate) => candidate == character);
      final query = repeated ? character : searchText;
      for (var offset = 1; offset <= widget.options.length; offset++) {
        final index =
            (active + offset + widget.options.length) % widget.options.length;
        final option = widget.options[index];
        if (option.enabled && option.label.toLowerCase().startsWith(query)) {
          setActive(index, reveal: true);
          break;
        }
      }
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  Widget _buildOverlay({
    required double left,
    required double top,
    required double width,
    required double maxHeight,
  }) => Positioned.fill(
    child: Stack(
      children: [
        Positioned.fill(
          child: GestureDetector(
            behavior: HitTestBehavior.translucent,
            onTapDown: (_) => close(restoreFocus: false),
          ),
        ),
        Positioned(
          left: left,
          top: top,
          width: width,
          child: FadeTransition(
            key: const ValueKey('floe-selection-fade'),
            opacity: CurvedAnimation(
              parent: popupAnimation,
              curve: FloeMotion.easeOut,
            ),
            child: ScaleTransition(
              key: const ValueKey('floe-selection-scale'),
              scale: Tween(begin: .97, end: 1.0).animate(
                CurvedAnimation(
                  parent: popupAnimation,
                  curve: FloeMotion.easeOut,
                ),
              ),
              alignment: Alignment.topCenter,
              child: Material(
                key: const ValueKey('floe-selection-popup'),
                color: FloePalette.neutral0,
                elevation: 8,
                shadowColor: FloePalette.neutral950.withValues(alpha: .08),
                shape: floeSquircleBorder(
                  FloeSquircleSize.md,
                  borderColor: FloePalette.neutral200,
                  borderWidth: 1,
                ),
                clipBehavior: Clip.antiAlias,
                child: ConstrainedBox(
                  constraints: BoxConstraints(maxHeight: maxHeight),
                  child: Focus(
                    focusNode: popupFocusNode,
                    onKeyEvent: navigate,
                    child: SingleChildScrollView(
                      padding: const EdgeInsets.all(5),
                      child: widget.options.isEmpty
                          ? const Padding(
                              padding: EdgeInsets.all(7),
                              child: Text(
                                'No options available',
                                style: TextStyle(color: FloePalette.neutral600),
                              ),
                            )
                          : Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                for (
                                  var index = 0;
                                  index < widget.options.length;
                                  index++
                                )
                                  _FloeSelectionOptionRow(
                                    key: optionKeys[index],
                                    option: widget.options[index],
                                    active: active == index,
                                    selected:
                                        !widget.menu &&
                                        widget.value ==
                                            widget.options[index].value,
                                    onHover: () => setActive(index),
                                    onPressed: () => choose(index),
                                  ),
                              ],
                            ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, _) {
      final trigger = _FloeSelectionTrigger(
        key: triggerKey,
        label: widget.label,
        value: widget.menu
            ? widget.label
            : selected?.label ?? widget.placeholder ?? '',
        enabled: widget.enabled,
        open: open,
        focusNode: focusNode,
        icon: widget.icon,
        description: widget.description,
        errorText: widget.errorText,
        field: !widget.menu,
        empty: selected == null,
        onPressed: toggle,
        onDirectionalOpen: (last) => show(last: last),
      );
      return widget.menu
          ? widget.icon == null
                ? trigger
                : Tooltip(message: widget.label, child: trigger)
          : trigger;
    },
  );
}

class _FloeSelectionOptionRow<T> extends StatelessWidget {
  const _FloeSelectionOptionRow({
    required this.option,
    required this.active,
    required this.selected,
    required this.onHover,
    required this.onPressed,
    super.key,
  });

  final FloeSelectOption<T> option;
  final bool active;
  final bool selected;
  final VoidCallback onHover;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    selected: selected,
    enabled: option.enabled,
    label: option.label,
    onTap: option.enabled ? onPressed : null,
    child: MouseRegion(
      cursor: option.enabled
          ? SystemMouseCursors.click
          : SystemMouseCursors.forbidden,
      onHover: (event) {
        if (event.kind == PointerDeviceKind.mouse && option.enabled) onHover();
      },
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: option.enabled ? onPressed : null,
        child: Container(
          key: const ValueKey('floe-selection-option'),
          constraints: const BoxConstraints(minHeight: 44),
          padding: const EdgeInsets.all(10),
          decoration: ShapeDecoration(
            color: active ? FloePalette.primary50 : Colors.transparent,
            shape: floeSquircleBorder(
              FloeSquircleSize.sm,
              borderColor: active ? FloePalette.primary600 : Colors.transparent,
              borderWidth: 2,
            ),
          ),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      option.label,
                      style: TextStyle(
                        color: option.enabled
                            ? FloePalette.neutral950
                            : FloePalette.neutral400,
                        fontSize: 14,
                        height: 1.35,
                      ),
                    ),
                    if (option.description != null) ...[
                      const SizedBox(height: 3),
                      Text(
                        option.description!,
                        style: const TextStyle(
                          color: FloePalette.neutral600,
                          fontSize: 12,
                          height: 1.4,
                        ),
                      ),
                    ],
                  ],
                ),
              ),
              if (selected) ...[
                const SizedBox(width: 16),
                const Icon(
                  LucideIcons.check,
                  size: 16,
                  color: FloePalette.primary600,
                ),
              ],
            ],
          ),
        ),
      ),
    ),
  );
}

class _FloeSelectionTrigger extends StatefulWidget {
  const _FloeSelectionTrigger({
    required this.value,
    required this.enabled,
    required this.open,
    required this.focusNode,
    required this.onPressed,
    required this.onDirectionalOpen,
    required this.field,
    required this.empty,
    this.label,
    this.icon,
    this.description,
    this.errorText,
    super.key,
  });

  final String? label;
  final String value;
  final bool enabled;
  final bool open;
  final FocusNode focusNode;
  final VoidCallback onPressed;
  final ValueChanged<bool> onDirectionalOpen;
  final Widget? icon;
  final bool field;
  final bool empty;
  final String? description;
  final String? errorText;

  @override
  State<_FloeSelectionTrigger> createState() => _FloeSelectionTriggerState();
}

class _FloeSelectionTriggerState extends State<_FloeSelectionTrigger> {
  bool hovered = false;
  bool focused = false;

  @override
  Widget build(BuildContext context) {
    final reduced = FloeMotion.reduceMotion(context);
    final active = widget.open || focused;
    final highlighted = active || (widget.enabled && hovered);
    final iconOnly = widget.icon != null;
    final fieldTextStyle = Theme.of(context).textTheme.bodyLarge;
    final value = Row(
      children: [
        Expanded(
          child: Text(
            widget.empty && !active ? '' : widget.value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: fieldTextStyle?.copyWith(
              color: widget.enabled
                  ? widget.empty
                        ? FloePalette.neutral500
                        : FloePalette.neutral950
                  : FloePalette.neutral500,
            ),
          ),
        ),
        const SizedBox(width: 16),
        const Icon(
          LucideIcons.chevronDown,
          size: 16,
          color: FloePalette.neutral600,
        ),
      ],
    );
    return Semantics(
      button: true,
      enabled: widget.enabled,
      expanded: widget.open,
      label: widget.label,
      value: widget.value,
      child: MouseRegion(
        cursor: widget.enabled
            ? SystemMouseCursors.click
            : SystemMouseCursors.forbidden,
        onEnter: (_) {
          if (widget.enabled) setState(() => hovered = true);
        },
        onExit: (_) => setState(() => hovered = false),
        child: Focus(
          focusNode: widget.focusNode,
          onFocusChange: (value) => setState(() => focused = value),
          onKeyEvent: (node, event) {
            if (event is KeyDownEvent &&
                (event.logicalKey == LogicalKeyboardKey.enter ||
                    event.logicalKey == LogicalKeyboardKey.space)) {
              widget.onPressed();
              return KeyEventResult.handled;
            }
            if (event is KeyDownEvent &&
                (event.logicalKey == LogicalKeyboardKey.arrowDown ||
                    event.logicalKey == LogicalKeyboardKey.arrowUp)) {
              widget.onDirectionalOpen(
                event.logicalKey == LogicalKeyboardKey.arrowUp,
              );
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          child: AnimatedContainer(
            key: const ValueKey('floe-selection-trigger'),
            duration: reduced ? Duration.zero : FloeMotion.hoverDuration,
            constraints: BoxConstraints(minWidth: iconOnly ? 44 : 0),
            decoration: widget.field
                ? ShapeDecoration(
                    color: Colors.transparent,
                    shape: floeSquircleBorder(FloeSquircleSize.md),
                  )
                : ShapeDecoration(
                    color: highlighted
                        ? FloePalette.neutral50
                        : FloePalette.neutral0,
                    shape: floeSquircleBorder(
                      FloeSquircleSize.md,
                      borderColor: focused
                          ? FloePalette.primary600
                          : highlighted
                          ? FloePalette.primary500
                          : FloePalette.neutral300,
                      borderWidth: focused ? 2 : 1,
                    ),
                  ),
            child: Material(
              color: Colors.transparent,
              shape: floeSquircleBorder(FloeSquircleSize.md),
              clipBehavior: Clip.antiAlias,
              child: InkWell(
                onTap: widget.enabled ? widget.onPressed : null,
                overlayColor: const WidgetStatePropertyAll(Colors.transparent),
                child: iconOnly
                    ? Padding(
                        padding: const EdgeInsets.all(11),
                        child: widget.icon,
                      )
                    : widget.field
                    ? InputDecorator(
                        key: const ValueKey('floe-selection-input-decorator'),
                        isFocused: active,
                        isHovering: widget.enabled && hovered,
                        isEmpty: widget.empty,
                        decoration: InputDecoration(
                          labelText: widget.label,
                          helperText: widget.description,
                          errorText: widget.errorText,
                          enabled: widget.enabled,
                        ),
                        child: value,
                      )
                    : Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 14,
                          vertical: 10,
                        ),
                        child: value,
                      ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class FloeCheckbox extends StatelessWidget {
  const FloeCheckbox({
    required this.value,
    required this.onChanged,
    this.semanticLabel,
    super.key,
  });

  final bool value;
  final ValueChanged<bool?>? onChanged;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) => _FloeChoice(
    selected: value,
    enabled: onChanged != null,
    semanticLabel: semanticLabel,
    onActivate: onChanged == null ? null : () => onChanged!(!value),
  );
}

class FloeCheckboxTile extends StatelessWidget {
  const FloeCheckboxTile({
    required this.value,
    required this.title,
    required this.onChanged,
    super.key,
  });

  final bool value;
  final Widget title;
  final ValueChanged<bool?>? onChanged;

  @override
  Widget build(BuildContext context) => _FloeChoice(
    selected: value,
    enabled: onChanged != null,
    title: title,
    onActivate: onChanged == null ? null : () => onChanged!(!value),
  );
}

class FloeRadioTile<T> extends StatefulWidget {
  const FloeRadioTile({
    required this.value,
    required this.title,
    this.enabled = true,
    super.key,
  });

  final T value;
  final Widget title;
  final bool enabled;

  @override
  State<FloeRadioTile<T>> createState() => _FloeRadioTileState<T>();
}

class _FloeRadioTileState<T> extends State<FloeRadioTile<T>>
    with RadioClient<T> {
  @override
  final FocusNode focusNode = FocusNode();

  @override
  bool get enabled => widget.enabled;

  @override
  T get radioValue => widget.value;

  @override
  bool get tristate => false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    registry = RadioGroup.maybeOf<T>(context);
    assert(registry != null, 'FloeRadioTile must be inside a RadioGroup<$T>.');
  }

  @override
  void dispose() {
    registry = null;
    focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => _FloeChoice(
    selected: registry?.groupValue == widget.value,
    enabled: widget.enabled,
    radio: true,
    title: widget.title,
    focusNode: focusNode,
    onActivate: widget.enabled ? () => registry?.onChanged(widget.value) : null,
  );
}

class _FloeChoice extends StatefulWidget {
  const _FloeChoice({
    required this.selected,
    required this.enabled,
    this.onActivate,
    this.title,
    this.semanticLabel,
    this.focusNode,
    this.radio = false,
  });

  final bool selected;
  final bool enabled;
  final VoidCallback? onActivate;
  final Widget? title;
  final String? semanticLabel;
  final FocusNode? focusNode;
  final bool radio;

  @override
  State<_FloeChoice> createState() => _FloeChoiceState();
}

class _FloeChoiceState extends State<_FloeChoice> {
  static _FloeChoiceState? hoverOwner;
  static _FloeChoiceState? focusOwner;

  FocusNode? internalFocusNode;
  FocusNode get focusNode =>
      widget.focusNode ?? (internalFocusNode ??= FocusNode());

  bool hovered = false;
  bool focused = false;

  void setHovered(bool value) {
    if (mounted && hovered != value) setState(() => hovered = value);
  }

  void claimHover() {
    if (hoverOwner == this) return;
    hoverOwner?.setHovered(false);
    hoverOwner = this;
    setHovered(true);
  }

  void setFocused(bool value) {
    if (value) {
      if (focusOwner != this) focusOwner?.setFocusStyle(false);
      focusOwner = this;
    } else if (focusOwner == this) {
      focusOwner = null;
    }
    setFocusStyle(value);
  }

  void setFocusStyle(bool value) {
    if (mounted && focused != value) setState(() => focused = value);
  }

  void clearPointerFocus() {
    focusOwner?.setFocusStyle(false);
    focusOwner = null;
    FocusManager.instance.primaryFocus?.unfocus();
  }

  @override
  void dispose() {
    if (hoverOwner == this) hoverOwner = null;
    if (focusOwner == this) focusOwner = null;
    internalFocusNode?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final visual = _FloeChoiceVisual(
      selected: widget.selected,
      enabled: widget.enabled,
      hovered: hovered,
      focused: focused,
      radio: widget.radio,
    );
    final content = widget.title == null
        ? SizedBox.square(dimension: 44, child: Center(child: visual))
        : ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 48),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
              child: Row(
                children: [
                  visual,
                  const SizedBox(width: 12),
                  Expanded(
                    child: DefaultTextStyle.merge(
                      style: TextStyle(
                        color: widget.enabled
                            ? FloePalette.neutral950
                            : FloePalette.neutral500,
                        fontSize: 13,
                        fontWeight: FontWeight.w500,
                        height: 1.5,
                      ),
                      child: widget.title!,
                    ),
                  ),
                ],
              ),
            ),
          );
    return Semantics(
      checked: widget.selected,
      enabled: widget.enabled,
      inMutuallyExclusiveGroup: widget.radio,
      label: widget.semanticLabel,
      onTap: widget.onActivate,
      child: MouseRegion(
        cursor: widget.enabled
            ? SystemMouseCursors.click
            : SystemMouseCursors.forbidden,
        onEnter: (event) {
          if (event.kind == PointerDeviceKind.mouse) claimHover();
        },
        onExit: (_) {
          if (hoverOwner == this) hoverOwner = null;
          setHovered(false);
        },
        child: FocusableActionDetector(
          enabled: widget.enabled,
          focusNode: focusNode,
          shortcuts: const {
            SingleActivator(LogicalKeyboardKey.space): ActivateIntent(),
            SingleActivator(LogicalKeyboardKey.enter): ActivateIntent(),
          },
          actions: {
            ActivateIntent: CallbackAction<ActivateIntent>(
              onInvoke: (_) {
                widget.onActivate?.call();
                return null;
              },
            ),
          },
          onShowFocusHighlight: setFocused,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapDown: widget.enabled ? (_) => clearPointerFocus() : null,
            onTap: widget.onActivate,
            child: Container(
              key: const ValueKey('floe-choice-hover-surface'),
              decoration: BoxDecoration(
                color: hovered && widget.enabled
                    ? FloePalette.primary50
                    : Colors.transparent,
                borderRadius: BorderRadius.circular(14),
              ),
              child: content,
            ),
          ),
        ),
      ),
    );
  }
}

class _FloeChoiceVisual extends StatelessWidget {
  const _FloeChoiceVisual({
    required this.selected,
    required this.enabled,
    required this.hovered,
    required this.focused,
    required this.radio,
  });

  final bool selected;
  final bool enabled;
  final bool hovered;
  final bool focused;
  final bool radio;

  @override
  Widget build(BuildContext context) {
    final reduced = FloeMotion.reduceMotion(context);
    final selectedColor = enabled
        ? FloePalette.primary600
        : FloePalette.neutral50;
    final borderColor = !enabled
        ? FloePalette.neutral200
        : selected || hovered
        ? FloePalette.primary600
        : FloePalette.neutral500;
    final markColor = enabled ? FloePalette.neutral0 : FloePalette.neutral300;
    final duration = reduced ? Duration.zero : FloeMotion.hoverDuration;
    return SizedBox.square(
      dimension: 20,
      child: Stack(
        clipBehavior: Clip.none,
        alignment: Alignment.center,
        children: [
          Positioned(
            left: -5,
            top: -5,
            width: 30,
            height: 30,
            child: AnimatedOpacity(
              key: const ValueKey('floe-choice-focus-ring'),
              opacity: focused && enabled ? 1 : 0,
              duration: duration,
              child: DecoratedBox(
                decoration: ShapeDecoration(
                  color: Colors.transparent,
                  shape: radio
                      ? const CircleBorder(
                          side: BorderSide(
                            color: FloePalette.primary600,
                            width: 2,
                          ),
                        )
                      : floeSquircleBorder(
                          FloeSquircleSize.sm,
                          borderColor: FloePalette.primary600,
                          borderWidth: 2,
                        ),
                ),
              ),
            ),
          ),
          AnimatedContainer(
            key: const ValueKey('floe-choice-visual'),
            width: 20,
            height: 20,
            duration: duration,
            curve: FloeMotion.easeOut,
            decoration: ShapeDecoration(
              color: selected ? selectedColor : FloePalette.neutral0,
              shadows: enabled
                  ? [
                      BoxShadow(
                        color: selected
                            ? FloePalette.primary600.withValues(alpha: .15)
                            : FloePalette.neutral950.withValues(alpha: .05),
                        offset: const Offset(0, 1),
                        blurRadius: 2,
                      ),
                    ]
                  : const [],
              shape: radio
                  ? CircleBorder(
                      side: BorderSide(color: borderColor, width: 1.5),
                    )
                  : floeSquircleBorder(
                      FloeSquircleSize.sm,
                      borderColor: borderColor,
                      borderWidth: 1.5,
                    ),
            ),
            child: AnimatedOpacity(
              opacity: selected ? 1 : 0,
              duration: reduced
                  ? Duration.zero
                  : const Duration(milliseconds: 120),
              child: Center(
                child: radio
                    ? Container(
                        width: 8,
                        height: 8,
                        decoration: BoxDecoration(
                          color: markColor,
                          shape: BoxShape.circle,
                        ),
                      )
                    : CustomPaint(
                        size: const Size.square(20),
                        painter: _FloeCheckPainter(markColor),
                      ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _FloeCheckPainter extends CustomPainter {
  const _FloeCheckPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.translate(-1.25, -.5);
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.2
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final path = Path()
      ..moveTo(5, 9.5)
      ..lineTo(8.2, 12.7)
      ..lineTo(15, 6);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_FloeCheckPainter oldDelegate) =>
      color != oldDelegate.color;
}
