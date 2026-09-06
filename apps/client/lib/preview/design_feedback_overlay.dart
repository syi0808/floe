import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class DesignFeedbackOverlay extends StatefulWidget {
  const DesignFeedbackOverlay({super.key, required this.child});

  final Widget child;

  @override
  State<DesignFeedbackOverlay> createState() => _DesignFeedbackOverlayState();
}

class _DesignFeedbackOverlayState extends State<DesignFeedbackOverlay> {
  final GlobalKey _surfaceKey = GlobalKey();
  final TextEditingController _commentController = TextEditingController();
  final List<_DesignAnnotation> _annotations = [];
  Timer? _statusTimer;
  bool _enabled = false;
  bool _selecting = false;
  _DesignTarget? _hoveredTarget;
  _DesignTarget? _draftTarget;
  int? _editingIndex;
  String? _status;

  @override
  void dispose() {
    _statusTimer?.cancel();
    _commentController.dispose();
    super.dispose();
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
    if (root == null) return null;
    _TargetCandidate? best;

    void visit(RenderObject object, int depth) {
      if (object is RenderBox && object.attached && object.hasSize) {
        try {
          final local = object.globalToLocal(position);
          if (object.size.contains(local)) {
            final topLeft = object.localToGlobal(Offset.zero);
            final bottomRight = object.localToGlobal(
              object.size.bottomRight(Offset.zero),
            );
            final rect = Rect.fromPoints(topLeft, bottomRight);
            if (rect.width > 1 && rect.height > 1) {
              final candidate = _TargetCandidate(
                depth: depth,
                area: rect.width * rect.height,
                target: _describeTarget(object, rect, position),
              );
              if (best == null ||
                  candidate.depth > best!.depth ||
                  (candidate.depth == best!.depth &&
                      candidate.area < best!.area)) {
                best = candidate;
              }
            }
          }
        } on Object {
          return;
        }
      }
      object.visitChildren((child) => visit(child, depth + 1));
    }

    visit(root, 0);
    return best?.target;
  }

  _DesignTarget _describeTarget(RenderObject object, Rect rect, Offset anchor) {
    final creator = object.debugCreator;
    final element = creator is DebugCreator ? creator.element : null;
    final widgets = <Widget>[];
    if (element != null) {
      widgets.add(element.widget);
      element.visitAncestorElements((ancestor) {
        widgets.add(ancestor.widget);
        return widgets.length < 12;
      });
    }
    final pathParts = widgets.reversed.map(_widgetName).fold(<String>[], (
      names,
      name,
    ) {
      if (names.isEmpty || names.last != name) names.add(name);
      return names;
    });
    final path = pathParts
        .skip(pathParts.length > 7 ? pathParts.length - 7 : 0)
        .join(' > ');
    final labelWidget = widgets.cast<Widget?>().firstWhere(
      (widget) =>
          widget != null && !_genericWidgets.contains(widget.runtimeType),
      orElse: () => widgets.firstOrNull,
    );
    return _DesignTarget(
      label: labelWidget == null
          ? object.runtimeType.toString()
          : _widgetName(labelWidget),
      path: path.isEmpty ? object.runtimeType.toString() : path,
      rect: rect,
      anchor: anchor,
      viewport: MediaQuery.sizeOf(context),
    );
  }

  String _widgetName(Widget widget) {
    final key = widget.key;
    if (key != null) return '${widget.runtimeType}[$key]';
    if (widget case Text(:final data?) when data.trim().isNotEmpty) {
      final singleLine = data.trim().replaceAll(RegExp(r'\s+'), ' ');
      final summary = singleLine.length > 32
          ? '${singleLine.substring(0, 29)}…'
          : singleLine;
      return 'Text “$summary”';
    }
    if (widget case Tooltip(:final message?) when message.trim().isNotEmpty) {
      return 'Tooltip “$message”';
    }
    return widget.runtimeType.toString();
  }

  void _saveDraft() {
    final target = _draftTarget;
    final comment = _commentController.text.trim();
    if (target == null || comment.isEmpty) return;
    setState(() {
      final annotation = _DesignAnnotation(
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
    await Clipboard.setData(ClipboardData(text: _markdownExport()));
    _showStatus('Markdown copied');
  }

  Future<void> _copyJson() async {
    final payload = {
      'version': 1,
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
        ..writeln('- Widget path: `${target.path.replaceAll('`', '\\`')}`')
        ..writeln(
          '- Position: (${target.anchor.dx.round()}, ${target.anchor.dy.round()}) logical px',
        )
        ..writeln(
          '- Bounds: ${target.rect.left.round()}, ${target.rect.top.round()}, '
          '${target.rect.width.round()} × ${target.rect.height.round()} logical px',
        )
        ..writeln(
          '- Viewport: ${target.viewport.width.round()} × ${target.viewport.height.round()} logical px',
        )
        ..writeln()
        ..writeln(annotation.comment)
        ..writeln();
    }
    return buffer.toString();
  }

  void _showStatus(String message) {
    _statusTimer?.cancel();
    setState(() => _status = message);
    _statusTimer = Timer(const Duration(seconds: 2), () {
      if (mounted) setState(() => _status = null);
    });
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {
      const SingleActivator(LogicalKeyboardKey.keyF, meta: true, shift: true):
          _toggle,
      const SingleActivator(LogicalKeyboardKey.escape): _cancelCurrentAction,
    },
    child: Focus(
      autofocus: true,
      child: Stack(
        fit: StackFit.expand,
        children: [
          KeyedSubtree(
            key: _surfaceKey,
            child: IgnorePointer(ignoring: _selecting, child: widget.child),
          ),
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
      ),
    ),
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
            const SizedBox(height: 12),
            TextField(
              key: const Key('design-feedback-comment'),
              controller: _commentController,
              autofocus: true,
              minLines: 3,
              maxLines: 6,
              decoration: const InputDecoration(
                labelText: 'Comment',
                hintText: 'Describe the visual or interaction change…',
                border: OutlineInputBorder(),
              ),
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
                style: const TextStyle(
                  color: Colors.white,
                  fontWeight: FontWeight.w700,
                ),
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

  Map<String, Object> toJson() => {
    'target': target.label,
    'widgetPath': target.path,
    'comment': comment,
    'createdAt': createdAt.toIso8601String(),
    'position': {'x': target.anchor.dx, 'y': target.anchor.dy},
    'bounds': {
      'left': target.rect.left,
      'top': target.rect.top,
      'width': target.rect.width,
      'height': target.rect.height,
    },
    'viewport': {
      'width': target.viewport.width,
      'height': target.viewport.height,
    },
  };
}

class _DesignTarget {
  const _DesignTarget({
    required this.label,
    required this.path,
    required this.rect,
    required this.anchor,
    required this.viewport,
  });

  final String label;
  final String path;
  final Rect rect;
  final Offset anchor;
  final Size viewport;

  _DesignTarget withAnchor(Offset value) => _DesignTarget(
    label: label,
    path: path,
    rect: rect,
    anchor: value,
    viewport: viewport,
  );
}

class _TargetCandidate {
  const _TargetCandidate({
    required this.depth,
    required this.area,
    required this.target,
  });

  final int depth;
  final double area;
  final _DesignTarget target;
}

const _genericWidgets = <Type>{
  Align,
  Center,
  ColoredBox,
  ConstrainedBox,
  DecoratedBox,
  Directionality,
  Expanded,
  Flexible,
  Padding,
  Positioned,
  RichText,
  SizedBox,
};

extension<T> on List<T> {
  T? get firstOrNull => isEmpty ? null : first;
}
