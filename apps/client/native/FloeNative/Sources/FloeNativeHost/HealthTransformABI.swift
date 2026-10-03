import Darwin
import Foundation
import FloeHealthTransformBridge

@_cdecl("floe_health_privacy_transform")
public func floeHealthPrivacyTransform(
    _ bytes: UnsafePointer<UInt8>?,
    _ length: Int
) -> UnsafeMutablePointer<CChar>? {
    let reply: HealthTransformReply
    if let bytes, (1...8_192).contains(length) {
        do {
            let command = try HealthTransformWireCodec.decodeCommand(Data(bytes: bytes, count: length))
            reply = NativeComposition.healthTransformHost.invoke(command)
        } catch {
            reply = HealthTransformReply(status: "error", failure: .invalidInput)
        }
    } else {
        reply = HealthTransformReply(status: "error", failure: .invalidInput)
    }

    guard let data = try? HealthTransformWireCodec.encodeReply(reply),
          data.count <= 4_096,
          let text = String(data: data, encoding: .utf8) else {
        return nil
    }
    return strdup(text)
}

@_cdecl("floe_health_privacy_transform_free")
public func floeHealthPrivacyTransformFree(_ output: UnsafeMutablePointer<CChar>?) {
    free(output)
}
