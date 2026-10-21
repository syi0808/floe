import FloeFoundationModels
import FloeModelExecution

// The dynamic image owns these instances. Runner links only stateless contract
// and source modules, so both ABI consumers reach one Health receipt registry.
enum NativeComposition {
    static let deviceModel: any DeviceModel = FoundationModelsDeviceModel()
    static let deviceModelHost = DeviceModelHost(model: deviceModel)
    static let healthTransformHost = HealthTransformHost(model: deviceModel)
}

// Live media lives in this bundled image so Rust reaches the same process-local
// lease owner that creates and stops each PeerConnection.
extension NativeComposition {
    static let liveMediaHost = LiveMediaHost(factory: WebRTCMediaPeerFactory())
}
