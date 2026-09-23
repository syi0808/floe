import Foundation

private var starts = 0

@_cdecl("floe_local_model")
public func invoke(
  _ input: UnsafePointer<UInt8>,
  _ length: Int
) -> UnsafeMutablePointer<CChar>? {
  let request = try! JSONSerialization.jsonObject(
    with: Data(bytes: input, count: length)
  ) as! [String: Any]
  let operation = request["operation"] as! String
  var response: [String: Any]
  if operation == "availability" {
    response = [
      "schemaVersion": 1,
      "status": "availability",
      "availability": "available",
    ]
  } else if operation == "start" {
    starts += 1
    let requestID = request["requestID"] as! String
    let step: [String: Any]
    switch starts {
    case 1:
      step = ["kind": "answer", "text": "Hello!"]
    case 2:
      step = [
        "kind": "call",
        "capabilityID": "floe.a2a.delegate",
        "input": "{\"agent_id\":\"floe.builtin.schedule\",\"message\":\"Read my calendar\",\"context_refs\":[]}",
      ]
    case 3:
      step = ["kind": "answer", "text": "Your calendar is clear."]
    default:
      step = ["kind": "answer", "text": "Your calendar is clear."]
    }
    response = [
      "schemaVersion": 1,
      "status": "done",
      "requestID": requestID,
      "step": step,
    ]
  } else {
    response = [
      "schemaVersion": 1,
      "status": "released",
      "requestID": request["requestID"]!,
    ]
  }
  let data = try! JSONSerialization.data(withJSONObject: response)
  return strdup(String(decoding: data, as: UTF8.self))
}

@_cdecl("floe_local_model_free")
public func release(_ value: UnsafeMutablePointer<CChar>?) {
  free(value)
}
