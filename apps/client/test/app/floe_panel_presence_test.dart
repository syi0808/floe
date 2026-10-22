import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_motion.dart';

void main() {
  testWidgets('reversing a closing panel retains exactly one mounted view', (
    tester,
  ) async {
    var mounts = 0;
    var disposals = 0;
    var visible = false;
    late StateSetter update;
    await tester.pumpWidget(
      MaterialApp(
        home: StatefulBuilder(
          builder: (context, setState) {
            update = setState;
            return FloePanelPresence(
              visible: visible,
              child: _Probe(
                onMount: () => mounts++,
                onDispose: () => disposals++,
              ),
            );
          },
        ),
      ),
    );
    expect(mounts, 0);
    update(() => visible = true);
    await tester.pumpAndSettle();
    expect(mounts, 1);
    update(() => visible = false);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    update(() => visible = true);
    await tester.pumpAndSettle();
    expect(mounts, 1);
    expect(disposals, 0);
    update(() => visible = false);
    await tester.pumpAndSettle();
    expect(disposals, 1);
    expect(tester.takeException(), isNull);
  });
  testWidgets('reduced motion opens and closes without an animation tail', (
    tester,
  ) async {
    var visible = false;
    late StateSetter update;
    await tester.pumpWidget(
      MaterialApp(
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: StatefulBuilder(
            builder: (context, setState) {
              update = setState;
              return FloePanelPresence(
                visible: visible,
                child: const Text('Panel'),
              );
            },
          ),
        ),
      ),
    );
    update(() => visible = true);
    await tester.pump();
    expect(find.text('Panel'), findsOneWidget);
    expect(tester.binding.hasScheduledFrame, isFalse);
    update(() => visible = false);
    await tester.pump();
    expect(find.text('Panel'), findsNothing);
    expect(tester.binding.hasScheduledFrame, isFalse);
  });
}

class _Probe extends StatefulWidget {
  const _Probe({required this.onMount, required this.onDispose});
  final VoidCallback onMount;
  final VoidCallback onDispose;
  @override
  State<_Probe> createState() => _ProbeState();
}

class _ProbeState extends State<_Probe> {
  @override
  void initState() {
    super.initState();
    widget.onMount();
  }

  @override
  void dispose() {
    widget.onDispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => const Text('Panel');
}
