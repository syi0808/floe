import 'package:flutter/material.dart';

import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/preview/design_system_catalog.dart';

void main() => runApp(
  MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: FloeTheme.light,
    home: const DesignSystemCatalog(),
  ),
);
