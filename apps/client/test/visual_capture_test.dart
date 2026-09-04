import 'dart:io';
import 'dart:ui' as ui;

import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_svg/flutter_svg.dart';

import 'package:floe_client/preview/prototype_fixture.dart';
import 'package:floe_client/preview/calendar_fixture.dart';

void main() {
  const output = String.fromEnvironment('VISUAL_OUTPUT');
  for (final size in [const Size(1440, 1100), const Size(390, 844)]) {
    for (final screen in ['today', 'event', 'connections', 'notes', 'task']) {
      testWidgets('$screen prototype capture ${size.width.toInt()}', (
        tester,
      ) async {
        debugDisableShadows = false;
        addTearDown(() => debugDisableShadows = true);
        tester.view.physicalSize = size;
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        if (output.isNotEmpty) {
          final font = File('assets/fonts/Pretendard-Regular.otf');
          final loader = FontLoader('Roboto');
          loader.addFont(
            Future.value(ByteData.sublistView(font.readAsBytesSync())),
          );
          await loader.load();
          final fallback = FontLoader('Ahem');
          fallback.addFont(
            Future.value(ByteData.sublistView(font.readAsBytesSync())),
          );
          await fallback.load();
          final pretendard = FontLoader('Pretendard');
          for (final weight in ['Regular', 'SemiBold', 'Bold']) {
            pretendard.addFont(
              Future.value(
                ByteData.sublistView(
                  File('assets/fonts/Pretendard-$weight.otf').readAsBytesSync(),
                ),
              ),
            );
          }
          await pretendard.load();
          final icons = FontLoader('MaterialIcons');
          icons.addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'));
          await tester.runAsync(icons.load);
          final lucide = FontLoader('packages/lucide_icons_flutter/Lucide');
          lucide.addFont(
            rootBundle.load('packages/lucide_icons_flutter/assets/lucide.ttf'),
          );
          await tester.runAsync(lucide.load);
        }
        final boundary = GlobalKey();
        await tester.pumpWidget(
          RepaintBoundary(
            key: boundary,
            child: prototypeAppearance(
              FloeApp(
                gateway: ['task', 'notes'].contains(screen)
                    ? prototypeGateway(detail: screen == 'task')
                    : calendarPreviewGateway(),
                query: ['task', 'notes'].contains(screen)
                    ? prototypeQuery
                    : calendarPreviewQuery,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(
          find.byWidgetPredicate(
            (widget) =>
                widget is FloeSquircle && widget.size == FloeSquircleSize.frame,
          ),
          findsOneWidget,
        );
        await tester.runAsync(() async {
          const loader = SvgAssetLoader('assets/floe-mascot.svg');
          await svg.cache.putIfAbsent(
            loader.cacheKey(null),
            () => loader.loadBytes(null),
          );
        });
        await tester.pumpAndSettle();
        if (screen == 'today') {
          final timeline = tester.getRect(
            find.byKey(const Key('timeline-card')),
          );
          expect(timeline.left, size.width <= 780 ? 12 : 136);
          expect(timeline.top, greaterThan(80));
          expect(timeline.height, greaterThan(400));
          expect(
            tester.getTopLeft(find.byKey(const Key('capture-field'))).dy,
            greaterThan(timeline.bottom),
          );
        }
        if (screen == 'event') {
          await tester.tap(find.text('Make room for the details'));
          await tester.pumpAndSettle();
        } else if (screen == 'connections') {
          await tester.tap(find.byTooltip('Connect'));
          await tester.pumpAndSettle();
        } else if (screen == 'notes') {
          await tester.tap(find.byTooltip('Notes'));
          await tester.pumpAndSettle();
        } else if (screen == 'task') {
          await tester.tap(find.byTooltip('Tasks'));
          await tester.pumpAndSettle();
          await tester.tap(
            find.widgetWithText(ListTile, 'Prepare launch brief'),
          );
          await tester.pumpAndSettle();
        }
        final layoutException = tester.takeException();
        if (output.isNotEmpty) {
          await tester.runAsync(() async {
            final render =
                boundary.currentContext!.findRenderObject()!
                    as RenderRepaintBoundary;
            final image = await render.toImage(pixelRatio: 1);
            final bytes = await image.toByteData(
              format: ui.ImageByteFormat.png,
            );
            await Directory(output).create(recursive: true);
            await File('$output/flutter-$screen-${size.width.toInt()}.png')
                .writeAsBytes(bytes!.buffer.asUint8List());
            image.dispose();
          });
        }
        debugDisableShadows = true;
        expect(layoutException, isNull);
      });
    }
  }
}
