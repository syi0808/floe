import Foundation

@_cdecl("floe_eventkit_action")
public func invoke(_ input: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
  let request = try! JSONSerialization.jsonObject(with: Data(String(cString: input).utf8)) as! [String: Any]
  let operation = request["operation"] as! String
  var response: [String: Any]
  if operation == "capabilities" {
    response = ["data": ["writes_enabled": true]]
  } else {
    let action = request["action"] as! [String: Any]
    let receipt: [String: Any] = [
      "execution_id": action["execution_id"]!, "person_id": action["person_id"]!,
      "provider": action["provider"]!, "calendar_id": action["calendar_id"]!,
      "title": action["title"]!, "schedule": action["schedule"]!, "external_id": "native-fixture-event|"
    ]
    if operation == "preflight" {
      response = ["data": [
        "person_id": action["person_id"]!, "provider": action["provider"]!,
        "calendar_id": action["calendar_id"]!, "permission_granted": true,
        "can_create": true, "timezone_valid": true, "has_conflict": action["title"] as? String == "Conflict"
      ]]
    } else if operation == "create" {
      let path = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().appendingPathComponent("creates.txt")
      let previous = (try? String(contentsOf: path, encoding: .utf8)) ?? ""
      try! (previous + "create\n").write(to: path, atomically: true, encoding: .utf8)
      response = action["title"] as? String == "Lost response" ? ["error": "timeout"] : ["data": receipt]
    } else {
      response = ["data": [receipt]]
    }
  }
  let data = try! JSONSerialization.data(withJSONObject: response)
  return strdup(String(decoding: data, as: UTF8.self))
}

@_cdecl("floe_eventkit_free")
public func release(_ value: UnsafeMutablePointer<CChar>?) { free(value) }
