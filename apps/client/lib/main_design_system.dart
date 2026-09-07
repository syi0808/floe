import 'package:flutter/material.dart';

import 'app/floe_theme.dart';
import 'preview/design_system_catalog.dart';

void main() => runApp(
  MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: FloeTheme.light,
    home: const DesignSystemCatalog(),
  ),
);
