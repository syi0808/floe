import Darwin
import Foundation

@_cdecl("floe_live_media")
public func floe_live_media(_ request: UnsafePointer<UInt8>?, _ length: Int) -> UnsafeMutablePointer<CChar>? {
    guard let request, length > 0, length <= 131_072 else { return nil }
    let reply = NativeComposition.liveMediaHost.invoke(Data(bytes: request, count: length))
    guard let text = String(data: reply, encoding: .utf8) else { return nil }
    return strdup(text)
}

@_cdecl("floe_live_media_free")
public func floe_live_media_free(_ value: UnsafeMutablePointer<CChar>?) {
    free(value)
}
