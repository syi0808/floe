/// App-lifetime service for one native callback lane.
///
/// The client bootstrap starts these services before Vault preparation and
/// retains them until app-lifetime shutdown.
abstract interface class NativeAcquisitionService {
  String get diagnosticOperation;

  Future<void> start();

  Future<void> dispose();
}
