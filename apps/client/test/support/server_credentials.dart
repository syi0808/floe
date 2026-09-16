import 'package:floe_client/features/connections/application/local_server_client.dart';

class MemoryServerCredentials implements ServerCredentialStore {
  String? value;
  @override
  Future<String?> read() async => value;
  @override
  Future<void> write(String data) async {
    value = data;
  }

  @override
  Future<void> delete() async {
    value = null;
  }
}
