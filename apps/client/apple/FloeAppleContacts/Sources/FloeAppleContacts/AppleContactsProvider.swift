import CryptoKit
import Foundation

public final class AppleContactsProvider {
    public static let maximumIdentityCount = 64
    static let maximumScanCount = 512
    static let freshnessMilliseconds: Int64 = 300_000

    private let store: AppleContactsStore
    private let handleKey: SymmetricKey
    private let now: () -> Date

    public convenience init(handleSecret: Data, now: @escaping () -> Date = Date.init) throws {
        try self.init(
            store: SystemAppleContactsStore(),
            handleSecret: handleSecret,
            now: now
        )
    }

    init(
        store: AppleContactsStore,
        handleSecret: Data,
        now: @escaping () -> Date = Date.init
    ) throws {
        guard handleSecret.count >= 32 else {
            throw AppleContactsProviderError.invalidHandleSecret
        }
        self.store = store
        handleKey = SymmetricKey(data: handleSecret)
        self.now = now
    }

    public func connectionSnapshot() -> AppleContactsConnectionSnapshot {
        AppleContactsConnectionSnapshot(authorization: store.authorizationState())
    }

    @discardableResult
    public func requestAuthorization() async throws -> AppleContactsConnectionSnapshot {
        guard store.authorizationState() == .notDetermined else {
            return connectionSnapshot()
        }
        do {
            _ = try await store.requestAuthorization()
        } catch {
            throw AppleContactsProviderError.authorizationRequestFailed
        }
        return connectionSnapshot()
    }

    public func readPeopleView(
        selection: AppleContactsSelection = .allAuthorized,
        limit: Int = AppleContactsProvider.maximumIdentityCount
    ) throws -> AppleContactsPeopleView {
        guard (1...Self.maximumIdentityCount).contains(limit) else {
            throw AppleContactsProviderError.invalidLimit
        }
        let authorization = store.authorizationState()
        guard authorization == .authorized || authorization == .limited else {
            throw AppleContactsProviderError.permissionRequired(authorization)
        }
        let selectedHandles: Set<String>?
        let scanLimit: Int
        switch selection {
        case .allAuthorized:
            selectedHandles = nil
            scanLimit = limit
        case let .identityHandles(handles):
            guard !handles.isEmpty,
                  handles.count <= Self.maximumIdentityCount,
                  handles.allSatisfy(Self.validHandle)
            else {
                throw AppleContactsProviderError.invalidSelection
            }
            selectedHandles = handles
            scanLimit = Self.maximumScanCount
        }
        let batch: AppleContactBatch
        do {
            batch = try store.fetchContacts(limit: scanLimit)
        } catch {
            throw AppleContactsProviderError.storeReadFailed
        }
        var identities = batch.records.compactMap(project)
        if let selectedHandles {
            identities = identities.filter { selectedHandles.contains($0.identityHandle) }
        }
        identities.sort {
            if $0.displayName == $1.displayName {
                return $0.identityHandle < $1.identityHandle
            }
            return $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
        }
        let outputWasTruncated = identities.count > limit
        identities = Array(identities.prefix(limit))
        let selectionResolved = selectedHandles.map {
            Set(identities.map(\.identityHandle)).isSuperset(of: $0)
        } ?? true
        let coverageComplete = selectedHandles == nil
            ? batch.coverageComplete && !outputWasTruncated
            : selectionResolved && !outputWasTruncated
        let observedAt = Int64(now().timeIntervalSince1970 * 1_000)
        return AppleContactsPeopleView(
            sourceHandle: opaqueHandle(prefix: "people:apple", value: "source"),
            observedAtUnixMilliseconds: observedAt,
            expiresAtUnixMilliseconds: observedAt + Self.freshnessMilliseconds,
            coverageComplete: coverageComplete,
            identities: identities
        )
    }

    private func project(_ record: AppleContactRecord) -> AppleContactIdentity? {
        let displayName = Self.boundedText(record.displayName, maximumBytes: 256)
        guard !displayName.isEmpty, !record.identifier.isEmpty else {
            return nil
        }
        var aliases: [String] = []
        Self.appendAlias(record.nickname, prefix: "name", to: &aliases)
        for email in record.emailAddresses {
            Self.appendAlias(email.lowercased(), prefix: "email", to: &aliases)
        }
        for phone in record.phoneNumbers {
            Self.appendAlias(Self.normalizedPhone(phone), prefix: "phone", to: &aliases)
        }
        let identityHandle = opaqueHandle(prefix: "person.identity", value: record.identifier)
        let evidenceHandle = opaqueHandle(prefix: "contact.evidence", value: record.identifier)
        return AppleContactIdentity(
            identityHandle: identityHandle,
            displayName: displayName,
            aliases: Array(aliases.prefix(8)),
            evidenceHandle: evidenceHandle
        )
    }

    private func opaqueHandle(prefix: String, value: String) -> String {
        let authentication = HMAC<SHA256>.authenticationCode(
            for: Data("\(prefix)\u{0}\(value)".utf8),
            using: handleKey
        )
        return "\(prefix):\(Data(authentication).map { String(format: "%02x", $0) }.joined().prefix(32))"
    }

    private static func appendAlias(_ value: String, prefix: String, to aliases: inout [String]) {
        guard aliases.count < 8 else { return }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return }
        let alias = boundedText("\(prefix):\(normalized)", maximumBytes: 256)
        guard !aliases.contains(alias) else { return }
        aliases.append(alias)
    }

    private static func normalizedPhone(_ value: String) -> String {
        let allowed = Set("+0123456789")
        return String(value.filter { allowed.contains($0) })
    }

    private static func boundedText(_ value: String, maximumBytes: Int) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.utf8.count > maximumBytes else { return trimmed }
        var result = ""
        for character in trimmed {
            guard (result + String(character)).utf8.count <= maximumBytes else { break }
            result.append(character)
        }
        return result
    }

    private static func validHandle(_ value: String) -> Bool {
        !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && value.utf8.count <= 128
    }
}
