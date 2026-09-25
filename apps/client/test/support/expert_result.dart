import 'dart:convert';
import 'dart:io';

Map<String, Object?> rustExpertResultFixture() => Map<String, Object?>.from(
  jsonDecode(
    File('../../fixtures/expert-result/schedule-v1.json').readAsStringSync(),
  ) as Map<String, dynamic>,
);
