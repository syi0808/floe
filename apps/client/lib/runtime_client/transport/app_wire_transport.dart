const appWireProtocolVersion = 2;

abstract interface class AppWireTransport {
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  });

  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  });

  Future<void> close();
}
