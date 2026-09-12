import Foundation

private var readCalls = 0

@_cdecl("floe_eventkit_action")
public func invoke(_ input: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
  let request = try! JSONSerialization.jsonObject(with: Data(String(cString: input).utf8)) as! [String: Any]
  let operation = request["operation"] as! String
  var response: [String: Any]
  if operation == "capabilities" {
    response = ["data": ["writes_enabled": true]]
  } else if operation == "view_access" {
    readCalls += 1
    if ProcessInfo.processInfo.environment["FLOE_NATIVE_READ_FIXTURE"] == "late" {
      Thread.sleep(forTimeInterval: 1)
    }
    let mode = ProcessInfo.processInfo.environment["FLOE_NATIVE_READ_FIXTURE"] ?? "valid"
    if mode == "malformed" {
      response = ["data": ["provider": "event_kit"]]
    } else {
      let generation = mode == "changed_generation" && readCalls > 1 ? "changed" : "stable"
      let identifiers = request["calendar_ids"] as! [String]
      response = ["data": [
        "schema_version": 1, "person_id": request["person_id"]!,
        "device_id": request["device_id"]!, "provider": "event_kit",
        "calendar_ids": identifiers.sorted(), "generation": generation
      ]]
    }
  } else if operation == "observe" {
    let mode = ProcessInfo.processInfo.environment["FLOE_NATIVE_READ_FIXTURE"] ?? "valid"
    let identifiers = request["calendar_ids"] as! [String]
    let generation = mode == "changed_generation" ? "stable" : "stable"
    let count = mode == "oversized" ? 129 : mode == "aggregate_oversized" ? 65 : 1
    let title = mode == "raw_oversized" ? String(repeating: "x", count: 70_000) : "Event"
    let records: [[String: Any]]
    if mode == "duplicate_record" {
      records = [[
        "can_modify": false, "calendar_id": identifiers[0],
        "external_id": "event", "external_revision": "revision", "title": title,
        "schedule": ["kind": "timed", "starts_at": request["starts_at"]!,
                      "ends_at": request["ends_at"]!, "timezone": "Etc/UTC"]
      ], [
        "can_modify": false, "calendar_id": identifiers[0],
        "external_id": "event", "external_revision": "revision", "title": title,
        "schedule": ["kind": "timed", "starts_at": request["starts_at"]!,
                      "ends_at": request["ends_at"]!, "timezone": "Etc/UTC"]
      ]]
    } else {
      records = (0..<count).map { index in
        [
          "can_modify": false, "calendar_id": identifiers[0],
          "external_id": "event-\(index)", "external_revision": "revision", "title": title,
          "schedule": ["kind": "timed", "starts_at": request["starts_at"]!,
                        "ends_at": request["ends_at"]!, "timezone": "Etc/UTC"]
        ]
      }
    }
    let batches: [[String: Any]]
    if mode == "duplicate_batch" {
      let duplicate = ["calendar_id": identifiers[0], "records": records, "failure": NSNull()] as [String: Any]
      batches = [duplicate, duplicate]
    } else {
      batches = identifiers.map { identifier in
        ["calendar_id": identifier, "records": records,
         "failure": mode == "partial" ? "calendar_unavailable" : NSNull()]
      }
    }
    response = ["data": [
      "stamp": ["schema_version": 1, "person_id": request["person_id"]!,
        "device_id": request["device_id"]!, "provider": "event_kit",
        "calendar_ids": identifiers.sorted(), "generation": generation],
      "observed_at": ISO8601DateFormatter().string(from: Date()),
      "batches": batches
    ]]
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
