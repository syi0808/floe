import Foundation

private typealias Invoke = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
private typealias Release = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
private let lock = NSLock()

@_cdecl("floe_eventkit_action")
public func invoke(_ input: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  lock.lock()
  defer { lock.unlock() }
  guard let input = input,
        let request = try? JSONSerialization.jsonObject(with: Data(String(cString: input).utf8)) as? [String: Any] else {
    return strdup("{\"error\":\"uncertain_result\"}")
  }
  if request["operation"] as? String == "create" {
    guard let action = request["action"] as? [String: Any],
          action["title"] as? String == "Floe S3 — disposable",
          action["calendar_name"] as? String == "iCloud · Floe Validation" else {
      return strdup("{\"error\":\"permission_denied\"}")
    }
  }
  let path = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent()
    .deletingLastPathComponent().appendingPathComponent("Frameworks/libfloe_eventkit_real.dylib").path
  guard let library = dlopen(path, RTLD_NOW),
        let callSymbol = dlsym(library, "floe_eventkit_action"),
        let freeSymbol = dlsym(library, "floe_eventkit_free") else {
    return strdup("{\"error\":\"provider_unavailable\"}")
  }
  let call = unsafeBitCast(callSymbol, to: Invoke.self)
  let release = unsafeBitCast(freeSymbol, to: Release.self)
  guard let result = call(input) else { return nil }
  let text = String(cString: result)
  release(result)
  if request["operation"] as? String == "create",
     let value = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any], value["data"] != nil {
    return strdup("{\"error\":\"timeout\"}")
  }
  return strdup(text)
}

@_cdecl("floe_eventkit_free")
public func release(_ value: UnsafeMutablePointer<CChar>?) { free(value) }
