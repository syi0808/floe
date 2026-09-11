import Cocoa
import CoreGraphics
import FlutterMacOS

struct MacOSAttentionProjection {
  let state: String
  let confidenceMillis: Int
  let evidenceHandles: [String]
}

final class MacOSAttentionReducer {
  static let maximumWindow: TimeInterval = 15 * 60
  static let minimumObservationWindow: TimeInterval = 45

  private let startedAt: Date
  private var activationTimes: [Date] = []
  private var sessionIsActive = true

  init(startedAt: Date = Date()) {
    self.startedAt = startedAt
  }

  func recordActivation(at date: Date) {
    activationTimes.append(date)
    prune(at: date)
  }

  func recordSessionActive(_ active: Bool, at date: Date) {
    sessionIsActive = active
    prune(at: date)
  }

  func projection(at date: Date, idleSeconds: TimeInterval) -> MacOSAttentionProjection {
    prune(at: date)
    if !sessionIsActive || idleSeconds >= 300 {
      return MacOSAttentionProjection(state: "unknown", confidenceMillis: 0, evidenceHandles: [])
    }
    guard date.timeIntervalSince(startedAt) >= Self.minimumObservationWindow else {
      return MacOSAttentionProjection(state: "unknown", confidenceMillis: 0, evidenceHandles: [])
    }
    let recentSwitches = activationTimes.filter { date.timeIntervalSince($0) <= 5 * 60 }.count
    if recentSwitches >= 6 {
      return MacOSAttentionProjection(
        state: "high_interruption_pressure",
        confidenceMillis: 800,
        evidenceHandles: ["attention.macos:switch_pressure", "attention.macos:recent_input"]
      )
    }
    if recentSwitches <= 2 && idleSeconds < 60 {
      return MacOSAttentionProjection(
        state: "focused",
        confidenceMillis: 750,
        evidenceHandles: ["attention.macos:stable_activity", "attention.macos:recent_input"]
      )
    }
    return MacOSAttentionProjection(
      state: "available",
      confidenceMillis: 650,
      evidenceHandles: ["attention.macos:mixed_activity"]
    )
  }

  var retainedActivationCount: Int {
    activationTimes.count
  }

  private func prune(at date: Date) {
    activationTimes.removeAll { date.timeIntervalSince($0) > Self.maximumWindow }
  }
}

final class MacOSAttentionBridge {
  private let reducer: MacOSAttentionReducer
  private let notificationCenter: NotificationCenter
  private var notificationTokens: [NSObjectProtocol] = []

  init(
    reducer: MacOSAttentionReducer = MacOSAttentionReducer(),
    workspace: NSWorkspace = .shared
  ) {
    self.reducer = reducer
    notificationCenter = workspace.notificationCenter
    observeWorkspace()
  }

  deinit {
    notificationTokens.forEach(notificationCenter.removeObserver)
  }

  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    guard call.method == "readAttention" else {
      result(FlutterMethodNotImplemented)
      return
    }
    let now = Date()
    let idleSeconds = CGEventSource.secondsSinceLastEventType(
      .combinedSessionState,
      eventType: .null
    )
    let projection = reducer.projection(at: now, idleSeconds: idleSeconds)
    let observedAt = Int64(now.timeIntervalSince1970 * 1000)
    result([
      "schema_version": 1,
      "view_id": "attention.coarse",
      "source_handle": "attention:macos_local",
      "observed_at_unix_ms": observedAt,
      "expires_at_unix_ms": observedAt + 60_000,
      "state": projection.state,
      "confidence_millis": projection.confidenceMillis,
      "evidence_handles": projection.evidenceHandles,
    ])
  }

  private func observeWorkspace() {
    notificationTokens.append(notificationCenter.addObserver(
      forName: NSWorkspace.didActivateApplicationNotification,
      object: nil,
      queue: .main
    ) { [weak self] _ in
      self?.reducer.recordActivation(at: Date())
    })
    notificationTokens.append(notificationCenter.addObserver(
      forName: NSWorkspace.sessionDidResignActiveNotification,
      object: nil,
      queue: .main
    ) { [weak self] _ in
      self?.reducer.recordSessionActive(false, at: Date())
    })
    notificationTokens.append(notificationCenter.addObserver(
      forName: NSWorkspace.sessionDidBecomeActiveNotification,
      object: nil,
      queue: .main
    ) { [weak self] _ in
      self?.reducer.recordSessionActive(true, at: Date())
    })
    notificationTokens.append(notificationCenter.addObserver(
      forName: NSWorkspace.willSleepNotification,
      object: nil,
      queue: .main
    ) { [weak self] _ in
      self?.reducer.recordSessionActive(false, at: Date())
    })
    notificationTokens.append(notificationCenter.addObserver(
      forName: NSWorkspace.didWakeNotification,
      object: nil,
      queue: .main
    ) { [weak self] _ in
      self?.reducer.recordSessionActive(true, at: Date())
    })
  }
}
