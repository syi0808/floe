import 'dart:convert';
import 'dart:io';
import 'package:path_provider/path_provider.dart';

/// An explicitly selected existing local profile. The Rust host validates its
/// database identity when opened; filesystem discovery is not authority.
final class ExistingLocalProfile {
  const ExistingLocalProfile._(this.personId, this.databasePath);
  final String personId;
  final String databasePath;
}

final class LocalProfileSelection {
  LocalProfileSelection._(this._support);
  final Directory _support;
  static Future<LocalProfileSelection> open() async => LocalProfileSelection._(await getApplicationSupportDirectory());
  File get _selection => File('${_support.path}/selected_profile.json');

  Future<ExistingLocalProfile?> selected() async {
    final type = await FileSystemEntity.type(_selection.path, followLinks: false);
    if (type == FileSystemEntityType.notFound) return null;
    if (type != FileSystemEntityType.file) throw const FormatException('The selected profile reference is invalid.');
    if (await _selection.length() > 1024) throw const FormatException('The selected profile reference is malformed.');
    final value = jsonDecode(await _selection.readAsString());
    if (value is! Map || value.length != 2 || value['schema_version'] != 1 || value['person_id'] is! String) {
      throw const FormatException('The selected profile reference is malformed.');
    }
    return _existing(value['person_id'] as String);
  }

  Future<List<ExistingLocalProfile>> candidates() async {
    final people = Directory('${_support.path}/people');
    final kind = await FileSystemEntity.type(people.path, followLinks: false);
    if (kind == FileSystemEntityType.notFound) return const [];
    if (kind != FileSystemEntityType.directory) throw const FormatException('The local profile directory is invalid.');
    final result = <ExistingLocalProfile>[];
    await for (final item in people.list(followLinks: false)) {
      if (item is! Directory) continue;
      final id = item.uri.pathSegments.where((part) => part.isNotEmpty).last;
      if (!_validPerson(id)) continue;
      final database = File('${item.path}/floe.db');
      if (await FileSystemEntity.type(database.path, followLinks: false) != FileSystemEntityType.file) continue;
      result.add(ExistingLocalProfile._(id, database.path));
    }
    result.sort((left, right) => left.personId.compareTo(right.personId));
    return List.unmodifiable(result);
  }

  Future<void> select(ExistingLocalProfile profile) async {
    final verified = await _existing(profile.personId);
    if (verified.databasePath != profile.databasePath) throw const FormatException('Profile location changed.');
    // This writes only the explicit user selection. It never creates/replaces a
    // profile, database, key or device identity.
    final temporary = File('${_selection.path}.tmp');
    await temporary.writeAsString(jsonEncode({'schema_version': 1, 'person_id': profile.personId}), flush: true);
    await temporary.rename(_selection.path);
  }

  Future<ExistingLocalProfile> _existing(String id) async {
    if (!_validPerson(id)) throw const FormatException('Invalid selected Person identity.');
    final directory = Directory('${_support.path}/people/$id');
    final database = File('${directory.path}/floe.db');
    if (await FileSystemEntity.type(directory.path, followLinks: false) != FileSystemEntityType.directory ||
        await FileSystemEntity.type(database.path, followLinks: false) != FileSystemEntityType.file) {
      throw const FormatException('The selected profile is missing or unreadable.');
    }
    return ExistingLocalProfile._(id, database.path);
  }

  static bool _validPerson(String value) =>
      value != '00000000-0000-0000-0000-000000000000' &&
      RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(value);
}
