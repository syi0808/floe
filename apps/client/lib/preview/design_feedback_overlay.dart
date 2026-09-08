import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';

import '../app/design_tokens.dart';
import '../app/floe_input.dart';

class DesignFeedbackOverlay extends StatefulWidget {
  const DesignFeedbackOverlay({
    super.key,
    required this.child,
    this.captureScreenshot,
  });

  final Widget child;
  final Future<String?> Function(Rect bounds)? captureScreenshot;

  @override
  State<DesignFeedbackOverlay> createState() => _DesignFeedbackOverlayState();
}

class _DesignFeedbackOverlayState extends State<DesignFeedbackOverlay> {
  static const _shortcutChannel = MethodChannel('floe/design-feedback');
  final GlobalKey _surfaceKey = GlobalKey();
  final TextEditingController _commentController = TextEditingController();
  final List<_DesignAnnotation> _annotations = [];
  final Set<Future<void>> _pendingCaptures = {};
  Timer? _statusTimer;
  bool _enabled = false;
  bool _selecting = false;
  _DesignTarget? _hoveredTarget;
  _DesignTarget? _draftTarget;
  int? _editingIndex;
  String? _status;

  @override
  void initState() {
    super.initState();
    HardwareKeyboard.instance.addHandler(_handleKeyEvent);
    _shortcutChannel.setMethodCallHandler(_handleShortcutCall);
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_handleKeyEvent);
    _shortcutChannel.setMethodCallHandler(null);
    _statusTimer?.cancel();
    _commentController.dispose();
    super.dispose();
  }

  Future<void> _handleShortcutCall(MethodCall call) async {
    if (call.method == 'toggle') _toggle();
  }

  bool _handleKeyEvent(KeyEvent event) {
    if (event is! KeyDownEvent) return false;
    final keyboard = HardwareKeyboard.instance;
    final isFeedbackShortcut =
        (event.logicalKey == LogicalKeyboardKey.keyF ||
            event.physicalKey == PhysicalKeyboardKey.keyF) &&
        keyboard.isMetaPressed &&
        keyboard.isShiftPressed;
    if (isFeedbackShortcut) {
      _toggle();
      return true;
    }
    if (event.logicalKey == LogicalKeyboardKey.escape &&
        (_selecting || _draftTarget != null)) {
      _cancelCurrentAction();
      return true;
    }
    return false;
  }

  void _toggle() {
    setState(() {
      _enabled = !_enabled;
      _selecting = false;
      _hoveredTarget = null;
      _draftTarget = null;
      _editingIndex = null;
      _commentController.clear();
    });
  }

  void _startSelecting() {
    setState(() {
      _enabled = true;
      _selecting = true;
      _hoveredTarget = null;
      _draftTarget = null;
      _editingIndex = null;
      _commentController.clear();
    });
  }

  void _cancelCurrentAction() {
    if (!_selecting && _draftTarget == null) return;
    setState(() {
      _selecting = false;
      _hoveredTarget = null;
      _draftTarget = null;
      _editingIndex = null;
      _commentController.clear();
    });
  }

  void _updateHoveredTarget(Offset position) {
    final target = _targetAt(position);
    if (target?.rect == _hoveredTarget?.rect &&
        target?.path == _hoveredTarget?.path) {
      return;
    }
    setState(() => _hoveredTarget = target);
  }

  void _selectTarget(TapDownDetails details) {
    final target = _targetAt(details.globalPosition);
    if (target == null) return;
    setState(() {
      _selecting = false;
      _hoveredTarget = null;
      _draftTarget = target.withAnchor(details.globalPosition);
      _editingIndex = null;
      _commentController.clear();
    });
  }

  _DesignTarget? _targetAt(Offset position) {
    final root = _surfaceKey.currentContext?.findRenderObject();
    if (root is! RenderBox || !root.hasSize) return null;
    final hitTest = BoxHitTestResult();
    root.hitTest(hitTest, position: root.globalToLocal(position));
    final object = hitTest.path
        .map((entry) => entry.target)
        .whereType<RenderBox>()
        .firstWhere(
          (candidate) => candidate.debugCreator is DebugCreator,
          orElse: () => root,
        );
    return _describeTarget(object, position);
  }

  _DesignTarget _describeTarget(RenderBox object, Offset anchor) {
    final creator = object.debugCreator;
    final element = creator is DebugCreator ? creator.element : null;
    final elements = <Element>[];
    if (element != null) {
      elements.add(element);
      element.visitAncestorElements((ancestor) {
        if (ancestor == _surfaceKey.currentContext ||
            ancestor.widget is DesignFeedbackOverlay) {
          return false;
        }
        elements.add(ancestor);
        return elements.length < 96;
      });
    }
    final identity = _identityElement(elements) ?? element;
    final identityObject = identity?.findRenderObject();
    final targetObject = identityObject is RenderBox && identityObject.hasSize
        ? identityObject
        : object;
    final rect = _globalRect(targetObject);
    final widgets = elements.map((candidate) => candidate.widget).toList();
    final pathParts = widgets.reversed.map(_widgetName).fold(<String>[], (
      names,
      name,
    ) {
      if (names.isEmpty || names.last != name) names.add(name);
      return names;
    });
    final path = pathParts.join(' > ');
    final identityWidget = identity?.widget;
    final text = identity == null ? null : _firstText(identity);
    final tooltip = identityWidget == null ? null : _tooltip(identityWidget);
    final semantics = identityWidget == null
        ? null
        : _semanticsLabel(identityWidget);
    final viewport = MediaQuery.sizeOf(context);
    final localPosition = Offset(anchor.dx - rect.left, anchor.dy - rect.top);
    return _DesignTarget(
      label: identityWidget == null
          ? object.runtimeType.toString()
          : _widgetName(identityWidget),
      path: path.isEmpty ? object.runtimeType.toString() : path,
      selector: _selector(elements, identity),
      widgetType: identityWidget?.runtimeType.toString() ?? 'unknown',
      widgetKey: identityWidget?.key?.toString(),
      text: text,
      tooltip: tooltip,
      semanticsLabel: semantics,
      componentPath: _componentPath(widgets),
      page: _pageName(widgets),
      route: identity == null ? null : ModalRoute.of(identity)?.settings.name,
      renderObjectType: targetObject.runtimeType.toString(),
      creatorChain: element?.debugGetCreatorChain(64) ?? '',
      contextLabels: identity == null ? const [] : _contextLabels(identity),
      scrollOffsets: _scrollOffsets(elements),
      rect: rect,
      anchor: anchor,
      localPosition: localPosition,
      normalizedPosition: Offset(
        viewport.width == 0 ? 0 : anchor.dx / viewport.width,
        viewport.height == 0 ? 0 : anchor.dy / viewport.height,
      ),
      viewport: viewport,
      devicePixelRatio: View.of(context).devicePixelRatio,
      platform: Theme.of(context).platform.name,
    );
  }

  Element? _identityElement(List<Element> elements) {
    Element? best;
    var bestScore = 0;
    for (var index = 0; index < elements.length; index++) {
      final widget = elements[index].widget;
      final score = _identityScore(widget) * 100 - index;
      if (score > bestScore) {
        best = elements[index];
        bestScore = score;
      }
    }
    return best;
  }

  int _identityScore(Widget widget) {
    if (_stableKey(widget) != null) return 10;
    if (_tooltip(widget) != null) return 9;
    if (widget is ButtonStyleButton || widget is IconButton) return 8;
    if (_semanticsLabel(widget) != null) return 8;
    if (widget is GestureDetector && widget.onTap != null) return 7;
    if (widget is Text && widget.data?.trim().isNotEmpty == true) return 6;
    if (_isComponent(widget)) return 5;
    if (widget is Slider || widget is Scrollbar) return 4;
    return 0;
  }

  Rect _globalRect(RenderBox object) => Rect.fromPoints(
    object.localToGlobal(Offset.zero),
    object.localToGlobal(object.size.bottomRight(Offset.zero)),
  );

  String? _tooltip(Widget widget) => switch (widget) {
    Tooltip(:final message?) when message.trim().isNotEmpty => message.trim(),
    IconButton(:final tooltip?) when tooltip.trim().isNotEmpty =>
      tooltip.trim(),
    _ => null,
  };

  String? _semanticsLabel(Widget widget) => switch (widget) {
    Semantics(:final properties)
        when properties.label?.trim().isNotEmpty == true =>
      properties.label!.trim(),
    _ => null,
  };

  String? _firstText(Element root) {
    String? result;
    void visit(Element element) {
      if (result != null) return;
      if (element.widget case Text(:final data?) when data.trim().isNotEmpty) {
        result = data.trim().replaceAll(RegExp(r'\s+'), ' ');
        return;
      }
      element.visitChildElements(visit);
    }

    visit(root);
    return result;
  }

  List<String> _contextLabels(Element root) {
    final labels = <String>[];
    void add(String? value) {
      final normalized = value?.trim().replaceAll(RegExp(r'\s+'), ' ');
      if (normalized != null &&
          normalized.isNotEmpty &&
          !labels.contains(normalized) &&
          labels.length < 8) {
        labels.add(normalized);
      }
    }

    void visit(Element element) {
      if (labels.length >= 8) return;
      final widget = element.widget;
      if (widget case Text(:final data?)) add(data);
      add(_tooltip(widget));
      add(_semanticsLabel(widget));
      element.visitChildElements(visit);
    }

    visit(root);
    return labels;
  }

  String _selector(List<Element> elements, Element? identity) {
    if (identity == null) return '';
    final identityIndex = elements.indexOf(identity);
    final selected = elements
        .skip(identityIndex)
        .where((element) => _identityScore(element.widget) > 0)
        .toList()
        .reversed
        .take(8)
        .toList();
    return selected
        .map((element) {
          final widget = element.widget;
          final key = _stableKey(widget);
          if (key != null) return '${widget.runtimeType}[${key.toString()}]';
          final tooltip = _tooltip(widget);
          if (tooltip != null) {
            return '${widget.runtimeType}[tooltip="$tooltip"]';
          }
          final semantics = _semanticsLabel(widget);
          if (semantics != null) {
            return '${widget.runtimeType}[label="$semantics"]';
          }
          if (widget case Text(:final data?) when data.trim().isNotEmpty) {
            return 'Text[text="${data.trim().replaceAll(RegExp(r'\s+'), ' ')}"]';
          }
          return widget.runtimeType.toString();
        })
        .join(' > ');
  }

  List<String> _componentPath(List<Widget> widgets) => widgets.reversed
      .where(_isComponent)
      .map((widget) => widget.runtimeType.toString())
      .fold(<String>[], (result, name) {
        if (result.isEmpty || result.last != name) result.add(name);
        return result;
      });

  String _pageName(List<Widget> widgets) => widgets
      .firstWhere(
        (widget) {
          final name = widget.runtimeType.toString();
          return name.endsWith('Screen') ||
              name.endsWith('Page') ||
              name.endsWith('Dialog') ||
              name.endsWith('Panel');
        },
        orElse: () {
          return widgets.firstWhere(
            (widget) => widget is Scaffold,
            orElse: () => widgets.firstOrNull ?? const SizedBox.shrink(),
          );
        },
      )
      .runtimeType
      .toString();

  Key? _stableKey(Widget widget) {
    final key = widget.key;
    return key is ValueKey<String> || key is ValueKey<int> ? key : null;
  }

  List<Map<String, Object>> _scrollOffsets(List<Element> elements) {
    final result = <Map<String, Object>>[];
    for (final element in elements) {
      if (element is StatefulElement && element.state is ScrollableState) {
        final state = element.state as ScrollableState;
        if (!state.position.hasPixels) continue;
        result.add({
          'axis': state.axisDirection.name,
          'pixels': state.position.pixels,
          'min': state.position.minScrollExtent,
          'max': state.position.maxScrollExtent,
        });
      }
    }
    return result;
  }

  bool _isComponent(Widget widget) {
    final name = widget.runtimeType.toString();
    return name.startsWith('Floe') ||
        name.startsWith('Calendar') ||
        name.startsWith('PersonalDay') ||
        name.startsWith('ReviewRequest') ||
        name.startsWith('Activity') ||
        name.startsWith('Connector') ||
        name.startsWith('Settings') ||
        (name.startsWith('_') &&
            const [
              'Screen',
              'Page',
              'Panel',
              'Toolbar',
              'Card',
              'Rail',
              'Agenda',
              'Notice',
              'Dialog',
            ].any(name.endsWith));
  }

  String _widgetName(Widget widget) {
    final key = widget.key;
    if (key != null) return '${widget.runtimeType}[$key]';
    final tooltip = _tooltip(widget);
    if (tooltip != null) return '${widget.runtimeType} “$tooltip”';
    final semantics = _semanticsLabel(widget);
    if (semantics != null) return '${widget.runtimeType} “$semantics”';
    if (widget case Text(:final data?) when data.trim().isNotEmpty) {
      final singleLine = data.trim().replaceAll(RegExp(r'\s+'), ' ');
      final summary = singleLine.length > 32
          ? '${singleLine.substring(0, 29)}…'
          : singleLine;
      return 'Text “$summary”';
    }
    return widget.runtimeType.toString();
  }

  void _saveDraft() {
    final target = _draftTarget;
    final comment = _commentController.text.trim();
    if (target == null || comment.isEmpty) return;
    late _DesignAnnotation annotation;
    setState(() {
      annotation = _DesignAnnotation(
        target: target,
        comment: comment,
        createdAt: _editingIndex == null
            ? DateTime.now()
            : _annotations[_editingIndex!].createdAt,
      );
      if (_editingIndex case final index?) {
        _annotations[index] = annotation;
      } else {
        _annotations.add(annotation);
      }
      _draftTarget = null;
      _editingIndex = null;
      _commentController.clear();
    });
    late final Future<void> capture;
    capture = _captureAnnotation(annotation)
        .whenComplete(() => _pendingCaptures.remove(capture));
    _pendingCaptures.add(capture);
  }

  Future<void> _captureAnnotation(_DesignAnnotation annotation) async {
    final capturedTarget = await _captureTarget(annotation.target);
    if (!mounted || capturedTarget.screenshotPath == null) return;
    final index = _annotations.indexWhere(
      (candidate) => candidate.createdAt == annotation.createdAt,
    );
    if (index < 0) return;
    setState(() {
      _annotations[index] = _DesignAnnotation(
        target: capturedTarget,
        comment: _annotations[index].comment,
        createdAt: _annotations[index].createdAt,
      );
    });
  }

  Future<_DesignTarget> _captureTarget(_DesignTarget target) async {
    try {
      if (widget.captureScreenshot case final capture?) {
        final path = await capture(target.rect);
        return path == null ? target : target.withScreenshot(path, target.rect);
      }
      final boundary = _surfaceKey.currentContext?.findRenderObject();
      if (boundary is! RenderRepaintBoundary || !boundary.hasSize) {
        return target;
      }
      final surfaceRect = _globalRect(boundary);
      final crop = target.rect
          .inflate(64)
          .intersect(surfaceRect)
          .shift(-surfaceRect.topLeft);
      if (crop.isEmpty) return target;
      final pixelRatio = target.devicePixelRatio.clamp(1.0, 2.0);
      final source = await boundary.toImage(pixelRatio: pixelRatio);
      final recorder = ui.PictureRecorder();
      final canvas = Canvas(recorder);
      final sourceRect = Rect.fromLTWH(
        crop.left * pixelRatio,
        crop.top * pixelRatio,
        crop.width * pixelRatio,
        crop.height * pixelRatio,
      );
      final destinationRect = Rect.fromLTWH(
        0,
        0,
        crop.width * pixelRatio,
        crop.height * pixelRatio,
      );
      canvas.drawImageRect(source, sourceRect, destinationRect, Paint());
      final image = await recorder.endRecording().toImage(
        destinationRect.width.ceil(),
        destinationRect.height.ceil(),
      );
      final data = await image.toByteData(format: ui.ImageByteFormat.png);
      source.dispose();
      image.dispose();
      if (data == null) return target;
      final directory = Directory(
        '${Directory.systemTemp.path}/floe-design-feedback',
      );
      await directory.create(recursive: true);
      final file = File(
        '${directory.path}/target-${DateTime.now().microsecondsSinceEpoch}.png',
      );
      await file.writeAsBytes(data.buffer.asUint8List(), flush: true);
      return target.withScreenshot(file.absolute.path, crop);
    } on Object {
      return target;
    }
  }

  void _editAnnotation(int index) {
    final annotation = _annotations[index];
    setState(() {
      _enabled = true;
      _selecting = false;
      _draftTarget = annotation.target;
      _editingIndex = index;
      _commentController.text = annotation.comment;
    });
  }

  void _deleteDraft() {
    final index = _editingIndex;
    setState(() {
      if (index != null) _annotations.removeAt(index);
      _draftTarget = null;
      _editingIndex = null;
      _commentController.clear();
    });
  }

  Future<void> _copyMarkdown() async {
    await Future.wait(_pendingCaptures.toList());
    if (!mounted) return;
    await Clipboard.setData(ClipboardData(text: _markdownExport()));
    _showStatus('Markdown copied');
  }

  Future<void> _copyJson() async {
    await Future.wait(_pendingCaptures.toList());
    if (!mounted) return;
    final payload = {
      'version': 2,
      'generatedAt': DateTime.now().toIso8601String(),
      'annotations': _annotations
          .map((annotation) => annotation.toJson())
          .toList(),
    };
    await Clipboard.setData(
      ClipboardData(text: const JsonEncoder.withIndent('  ').convert(payload)),
    );
    _showStatus('JSON copied');
  }

  String _markdownExport() {
    final buffer = StringBuffer()
      ..writeln('# Floe design feedback')
      ..writeln()
      ..writeln('Generated: ${DateTime.now().toIso8601String()}')
      ..writeln();
    for (var index = 0; index < _annotations.length; index++) {
      final annotation = _annotations[index];
      final target = annotation.target;
      buffer
        ..writeln('## ${index + 1}. ${target.label}')
        ..writeln()
        ..writeln('- Page: `${_markdownCode(target.page)}`')
        ..writeln('- Selector: `${_markdownCode(target.selector)}`')
        ..writeln(
          '- Components: `${_markdownCode(target.componentPath.join(' > '))}`',
        )
        ..writeln('- Widget path: `${_markdownCode(target.path)}`')
        ..writeln(
          '- Render object: `${_markdownCode(target.renderObjectType)}`',
        )
        ..writeln('- Identifiers: ${_identifierSummary(target)}')
        ..writeln(
          '- Context: ${target.contextLabels.isEmpty ? 'none' : target.contextLabels.map((label) => '`${_markdownCode(label)}`').join(', ')}',
        )
        ..writeln(
          '- Position: (${target.anchor.dx.round()}, ${target.anchor.dy.round()}) logical px',
        )
        ..writeln(
          '- Local position: (${target.localPosition.dx.round()}, ${target.localPosition.dy.round()}); '
          'normalized: (${target.normalizedPosition.dx.toStringAsFixed(4)}, '
          '${target.normalizedPosition.dy.toStringAsFixed(4)})',
        )
        ..writeln(
          '- Bounds: ${target.rect.left.round()}, ${target.rect.top.round()}, '
          '${target.rect.width.round()} × ${target.rect.height.round()} logical px',
        )
        ..writeln(
          '- Viewport: ${target.viewport.width.round()} × ${target.viewport.height.round()} logical px; '
          'DPR: ${target.devicePixelRatio}; platform: ${target.platform}',
        );
      if (target.scrollOffsets.isNotEmpty) {
        buffer.writeln(
          '- Scroll offsets: `${_markdownCode(jsonEncode(target.scrollOffsets))}`',
        );
      }
      if (target.route case final route?) {
        buffer.writeln('- Route: `${_markdownCode(route)}`');
      }
      if (target.screenshotPath case final screenshot?) {
        buffer
          ..writeln('- Screenshot: `${_markdownCode(screenshot)}`')
          ..writeln()
          ..writeln('![Selected element context]($screenshot)');
      }
      buffer
        ..writeln()
        ..writeln(annotation.comment)
        ..writeln();
    }
    return buffer.toString();
  }

  String _markdownCode(String value) => value.replaceAll('`', '\\`');

  String _identifierSummary(_DesignTarget target) {
    final values = <String>['type `${_markdownCode(target.widgetType)}`'];
    if (target.widgetKey case final key?) {
      values.add('key `${_markdownCode(key)}`');
    }
    if (target.text case final text?) {
      values.add('text `${_markdownCode(text)}`');
    }
    if (target.tooltip case final tooltip?) {
      values.add('tooltip `${_markdownCode(tooltip)}`');
    }
    if (target.semanticsLabel case final label?) {
      values.add('semantics `${_markdownCode(label)}`');
    }
    return values.join('; ');
  }

  void _showStatus(String message) {
    _statusTimer?.cancel();
    setState(() => _status = message);
    _statusTimer = Timer(const Duration(seconds: 2), () {
      if (mounted) setState(() => _status = null);
    });
  }

  @override
  Widget build(BuildContext context) => Stack(
    fit: StackFit.expand,
    children: [
      RepaintBoundary(key: _surfaceKey, child: widget.child),
      if (_enabled)
        for (var index = 0; index < _annotations.length; index++)
          _FeedbackPin(
            index: index,
            annotation: _annotations[index],
            onPressed: () => _editAnnotation(index),
          ),
      if (_enabled && _hoveredTarget != null)
        _TargetHighlight(target: _hoveredTarget!),
      if (_selecting)
        Positioned.fill(
          child: MouseRegion(
            cursor: SystemMouseCursors.precise,
            onHover: (event) => _updateHoveredTarget(event.position),
            onExit: (_) => setState(() => _hoveredTarget = null),
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTapDown: _selectTarget,
            ),
          ),
        ),
      if (_enabled) Positioned(top: 12, right: 12, child: _buildToolbar()),
      if (_draftTarget != null)
        Positioned(top: 76, right: 12, child: _buildEditor()),
    ],
  );

  Widget _buildToolbar() => Material(
    key: const ValueKey('design-feedback-toolbar-expanded'),
    elevation: 8,
    borderRadius: BorderRadius.circular(16),
    clipBehavior: Clip.antiAlias,
    child: Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextButton.icon(
            onPressed: _selecting ? _cancelCurrentAction : _startSelecting,
            icon: Icon(_selecting ? Icons.close : Icons.ads_click, size: 18),
            label: Text(_selecting ? 'Cancel' : 'Inspect'),
          ),
          const SizedBox(width: 4),
          Text('${_annotations.length} pins'),
          const SizedBox(width: 4),
          IconButton(
            tooltip: 'Copy Markdown',
            onPressed: _annotations.isEmpty ? null : _copyMarkdown,
            icon: const Icon(Icons.description_outlined),
          ),
          IconButton(
            tooltip: 'Copy JSON',
            onPressed: _annotations.isEmpty ? null : _copyJson,
            icon: const Icon(Icons.data_object),
          ),
          IconButton(
            tooltip: 'Clear feedback',
            onPressed: _annotations.isEmpty
                ? null
                : () => setState(_annotations.clear),
            icon: const Icon(Icons.delete_sweep_outlined),
          ),
          IconButton(
            tooltip: 'Close design feedback',
            onPressed: _toggle,
            icon: const Icon(Icons.close),
          ),
          if (_status case final status?) ...[
            const SizedBox(width: 4),
            Text(status, key: const Key('design-feedback-status')),
            const SizedBox(width: 8),
          ],
        ],
      ),
    ),
  );

  Widget _buildEditor() => Material(
    elevation: 10,
    borderRadius: BorderRadius.circular(20),
    clipBehavior: Clip.antiAlias,
    child: SizedBox(
      width: 360,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              _editingIndex == null ? 'Add feedback' : 'Edit feedback',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 4),
            Text(
              _draftTarget!.label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodySmall,
            ),
            if (_draftTarget!.selector.isNotEmpty) ...[
              const SizedBox(height: 4),
              Text(
                _draftTarget!.selector,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
            const SizedBox(height: 12),
            FloeInput(
              key: const Key('design-feedback-comment'),
              label: 'Comment',
              controller: _commentController,
              autofocus: true,
              minLines: 3,
              maxLines: 6,
              placeholder: 'Describe the visual or interaction change…',
              onSubmitted: (_) => _saveDraft(),
            ),
            const SizedBox(height: 12),
            Wrap(
              alignment: WrapAlignment.end,
              spacing: 8,
              runSpacing: 8,
              children: [
                if (_editingIndex != null)
                  TextButton(
                    onPressed: _deleteDraft,
                    child: const Text('Delete'),
                  ),
                TextButton(
                  onPressed: _cancelCurrentAction,
                  child: const Text('Cancel'),
                ),
                FilledButton(
                  onPressed: _saveDraft,
                  child: const Text('Save pin'),
                ),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}

class _FeedbackPin extends StatelessWidget {
  const _FeedbackPin({
    required this.index,
    required this.annotation,
    required this.onPressed,
  });

  final int index;
  final _DesignAnnotation annotation;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => Positioned(
    left: annotation.target.anchor.dx - 15,
    top: annotation.target.anchor.dy - 15,
    child: Tooltip(
      message: annotation.comment,
      child: Material(
        color: const Color(0xff6d4aff),
        shape: const CircleBorder(),
        elevation: 4,
        child: InkWell(
          key: ValueKey('design-feedback-pin-${index + 1}'),
          customBorder: const CircleBorder(),
          onTap: onPressed,
          child: SizedBox.square(
            dimension: 30,
            child: Center(
              child: Text(
                '${index + 1}',
                style: FloeType.label.copyWith(color: Colors.white),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

class _TargetHighlight extends StatelessWidget {
  const _TargetHighlight({required this.target});

  final _DesignTarget target;

  @override
  Widget build(BuildContext context) => Positioned.fromRect(
    rect: target.rect,
    child: IgnorePointer(
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: const Color(0x196d4aff),
          border: Border.all(color: const Color(0xff6d4aff), width: 2),
        ),
      ),
    ),
  );
}

class _DesignAnnotation {
  const _DesignAnnotation({
    required this.target,
    required this.comment,
    required this.createdAt,
  });

  final _DesignTarget target;
  final String comment;
  final DateTime createdAt;

  Map<String, Object?> toJson() => {
    'target': target.label,
    'page': target.page,
    'route': target.route,
    'selector': target.selector,
    'components': target.componentPath,
    'widgetPath': target.path,
    'creatorChain': target.creatorChain,
    'renderObject': target.renderObjectType,
    'identifiers': {
      'widgetType': target.widgetType,
      'key': target.widgetKey,
      'text': target.text,
      'tooltip': target.tooltip,
      'semanticsLabel': target.semanticsLabel,
    },
    'contextLabels': target.contextLabels,
    'comment': comment,
    'createdAt': createdAt.toIso8601String(),
    'position': {
      'x': target.anchor.dx,
      'y': target.anchor.dy,
      'localX': target.localPosition.dx,
      'localY': target.localPosition.dy,
      'normalizedX': target.normalizedPosition.dx,
      'normalizedY': target.normalizedPosition.dy,
    },
    'bounds': {
      'left': target.rect.left,
      'top': target.rect.top,
      'width': target.rect.width,
      'height': target.rect.height,
    },
    'viewport': {
      'width': target.viewport.width,
      'height': target.viewport.height,
      'devicePixelRatio': target.devicePixelRatio,
      'platform': target.platform,
    },
    'scrollOffsets': target.scrollOffsets,
    'screenshot': target.screenshotPath == null
        ? null
        : {
            'path': target.screenshotPath,
            'crop': {
              'left': target.screenshotCrop?.left,
              'top': target.screenshotCrop?.top,
              'width': target.screenshotCrop?.width,
              'height': target.screenshotCrop?.height,
            },
          },
  };
}

class _DesignTarget {
  const _DesignTarget({
    required this.label,
    required this.path,
    required this.selector,
    required this.widgetType,
    required this.widgetKey,
    required this.text,
    required this.tooltip,
    required this.semanticsLabel,
    required this.componentPath,
    required this.page,
    required this.route,
    required this.renderObjectType,
    required this.creatorChain,
    required this.contextLabels,
    required this.scrollOffsets,
    required this.rect,
    required this.anchor,
    required this.localPosition,
    required this.normalizedPosition,
    required this.viewport,
    required this.devicePixelRatio,
    required this.platform,
    this.screenshotPath,
    this.screenshotCrop,
  });

  final String label;
  final String path;
  final String selector;
  final String widgetType;
  final String? widgetKey;
  final String? text;
  final String? tooltip;
  final String? semanticsLabel;
  final List<String> componentPath;
  final String page;
  final String? route;
  final String renderObjectType;
  final String creatorChain;
  final List<String> contextLabels;
  final List<Map<String, Object>> scrollOffsets;
  final Rect rect;
  final Offset anchor;
  final Offset localPosition;
  final Offset normalizedPosition;
  final Size viewport;
  final double devicePixelRatio;
  final String platform;
  final String? screenshotPath;
  final Rect? screenshotCrop;

  _DesignTarget withAnchor(Offset value) => _DesignTarget(
    label: label,
    path: path,
    selector: selector,
    widgetType: widgetType,
    widgetKey: widgetKey,
    text: text,
    tooltip: tooltip,
    semanticsLabel: semanticsLabel,
    componentPath: componentPath,
    page: page,
    route: route,
    renderObjectType: renderObjectType,
    creatorChain: creatorChain,
    contextLabels: contextLabels,
    scrollOffsets: scrollOffsets,
    rect: rect,
    anchor: value,
    localPosition: Offset(value.dx - rect.left, value.dy - rect.top),
    normalizedPosition: Offset(
      viewport.width == 0 ? 0 : value.dx / viewport.width,
      viewport.height == 0 ? 0 : value.dy / viewport.height,
    ),
    viewport: viewport,
    devicePixelRatio: devicePixelRatio,
    platform: platform,
    screenshotPath: screenshotPath,
    screenshotCrop: screenshotCrop,
  );

  _DesignTarget withScreenshot(String path, Rect crop) => _DesignTarget(
    label: label,
    path: this.path,
    selector: selector,
    widgetType: widgetType,
    widgetKey: widgetKey,
    text: text,
    tooltip: tooltip,
    semanticsLabel: semanticsLabel,
    componentPath: componentPath,
    page: page,
    route: route,
    renderObjectType: renderObjectType,
    creatorChain: creatorChain,
    contextLabels: contextLabels,
    scrollOffsets: scrollOffsets,
    rect: rect,
    anchor: anchor,
    localPosition: localPosition,
    normalizedPosition: normalizedPosition,
    viewport: viewport,
    devicePixelRatio: devicePixelRatio,
    platform: platform,
    screenshotPath: path,
    screenshotCrop: crop,
  );
}

extension<T> on List<T> {
  T? get firstOrNull => isEmpty ? null : first;
}
