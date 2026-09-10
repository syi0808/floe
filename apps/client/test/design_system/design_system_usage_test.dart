import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('feature UI uses Floe design-system components and typography', () {
    final featureFiles = Directory('lib/features')
        .listSync(recursive: true)
        .whereType<File>()
        .where((file) => file.path.endsWith('.dart'));
    final stockVisual = RegExp(
      r'\b(?:Scaffold|AppBar|AlertDialog|Dialog|TextButton|OutlinedButton|FilledButton|IconButton|Checkbox(?:ListTile)?|Radio(?:ListTile|Group)?|Switch(?:ListTile)?|TextField|TextFormField|Slider|Divider|ListTile|Card|Chip|Badge|Material|InkWell|Tooltip)\s*(?:\.|<|\()',
    );
    final violations = <String>[];

    for (final file in featureFiles) {
      final source = file.readAsStringSync();
      if (source.contains('TextStyle(')) {
        violations.add('${file.path}: direct TextStyle');
      }
      if (stockVisual.hasMatch(source)) {
        violations.add('${file.path}: stock visual component');
      }
    }

    expect(violations, isEmpty, reason: violations.join('\n'));
  });
}
