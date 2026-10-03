import CryptoKit
import Foundation

public final class AppleContactsProvider {
    public static let maximumIdentityCount = 64
    public static let maximumSerializedViewBytes = 32_768
    static let maximumScanCount = 512
    static let freshnessMilliseconds: Int64 = 300_000

    private let store: AppleContactsStore
    private let handleKey: SymmetricKey
    private let now: () -> Date
    private var nativeIdentifiersByHandle: [String: String] = [:]

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

    /// Resource selection metadata only. Phone numbers, email addresses and
    /// aliases are never fetched before a source read is admitted.
    public func inspectCatalog(limit: Int = 256) throws -> AppleContactsCatalog {
        guard (1...Self.maximumScanCount).contains(limit) else { throw AppleContactsProviderError.invalidLimit }
        let authorization = store.authorizationState()
        guard authorization == .authorized || authorization == .limited else {
            throw AppleContactsProviderError.permissionRequired(authorization)
        }
        let batch: AppleContactResourceBatch
        do { batch = try store.fetchResourceMetadata(limit: limit, identifiers: nil) }
        catch { throw AppleContactsProviderError.storeReadFailed }
        guard store.authorizationState() == authorization else {
            throw AppleContactsProviderError.permissionRequired(store.authorizationState())
        }
        var resources: [AppleContactResource] = []
        var coverageComplete = batch.coverageComplete
        for record in batch.records {
            guard !record.identifier.isEmpty else { throw AppleContactsProviderError.invalidSelection }
            let label = Self.boundedText(record.displayName, maximumBytes: 256)
            guard !label.isEmpty, !label.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else {
                throw AppleContactsProviderError.invalidSelection
            }
            let handle = opaqueHandle(prefix: "person.identity", value: record.identifier)
            nativeIdentifiersByHandle[handle] = record.identifier
            let resource = AppleContactResource(handle: handle, label: label)
            let candidate = resources + [resource]
            if (try JSONEncoder().encode(candidate)).count > 24_576 {
                coverageComplete = false
                break
            }
            resources.append(resource)
        }
        resources.sort { $0.handle < $1.handle }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let encoded = try encoder.encode(resources)
        var fingerprintInput = Data("contacts.catalog\0\(authorization.rawValue)\0\(coverageComplete)\0".utf8)
        fingerprintInput.append(encoded)
        let digest = HMAC<SHA256>.authenticationCode(for: fingerprintInput, using: handleKey)
        return AppleContactsCatalog(resources: resources,
            nativeSubjectFingerprint: digest.map { String(format: "%02x", $0) }.joined(),
            permissionClass: authorization.rawValue, coverageComplete: coverageComplete)
    }

    private func resolveNativeIdentifiers(_ handles: Set<String>) throws -> Set<String> {
        // Re-establish the native mapping from real metadata after process restart.
        // This never infers that a missing bounded row was revoked.
        if handles.contains(where: { nativeIdentifiersByHandle[$0] == nil }) {
            let authorization = store.authorizationState()
            guard authorization == .authorized || authorization == .limited else {
                throw AppleContactsProviderError.permissionRequired(authorization)
            }
            let batch: AppleContactResourceBatch
            do { batch = try store.fetchResourceMetadata(limit: Self.maximumScanCount, identifiers: nil) }
            catch { throw AppleContactsProviderError.storeReadFailed }
            guard store.authorizationState() == authorization else {
                throw AppleContactsProviderError.permissionRequired(store.authorizationState())
            }
            for record in batch.records {
                let handle = opaqueHandle(prefix: "person.identity", value: record.identifier)
                nativeIdentifiersByHandle[handle] = record.identifier
            }
        }
        let identifiers = Set(handles.compactMap { nativeIdentifiersByHandle[$0] })
        guard identifiers.count == handles.count else { throw AppleContactsProviderError.selectionUnresolved }
        return identifiers
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
        let selectedIdentifiers: Set<String>?
        let scanLimit: Int
        switch selection {
        case .allAuthorized:
            selectedHandles = nil
            selectedIdentifiers = nil
            scanLimit = limit
        case let .identityHandles(handles):
            guard !handles.isEmpty,
                  handles.count <= Self.maximumIdentityCount,
                  handles.allSatisfy(Self.validHandle)
            else {
                throw AppleContactsProviderError.invalidSelection
            }
            selectedHandles = handles
            selectedIdentifiers = try resolveNativeIdentifiers(handles)
            scanLimit = Self.maximumScanCount
        }
        let batch: AppleContactBatch
        do {
            batch = try store.fetchContacts(limit: scanLimit, identifiers: selectedIdentifiers)
        } catch {
            throw AppleContactsProviderError.storeReadFailed
        }
        let authorizationAfterRead = store.authorizationState()
        guard authorizationAfterRead == .authorized || authorizationAfterRead == .limited else {
            throw AppleContactsProviderError.permissionRequired(authorizationAfterRead)
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
        return budgetedView(
            sourceHandle: opaqueHandle(prefix: "people:apple", value: "source"),
            observedAtUnixMilliseconds: observedAt,
            expiresAtUnixMilliseconds: observedAt + Self.freshnessMilliseconds,
            requestedCoverageComplete: coverageComplete,
            identities: identities
        )
    }

    public func inspectSelectedSubject(_ handles: [String]) throws -> AppleContactsSubject {
        guard !handles.isEmpty,
              handles.count <= Self.maximumIdentityCount,
              Set(handles).count == handles.count,
              handles.allSatisfy(Self.validHandle)
        else { throw AppleContactsProviderError.selectionUnresolved }
        let identifiers = try resolveNativeIdentifiers(Set(handles))
        let authorization = store.authorizationState()
        guard authorization == .authorized || authorization == .limited else {
            throw AppleContactsProviderError.permissionRequired(authorization)
        }
        let batch: AppleContactResourceBatch
        do {
            batch = try store.fetchResourceMetadata(limit: handles.count, identifiers: identifiers)
        } catch {
            throw AppleContactsProviderError.storeReadFailed
        }
        guard store.authorizationState() == authorization else {
            throw AppleContactsProviderError.permissionRequired(store.authorizationState())
        }
        let resolved = Set(batch.records.map(\.identifier))
        guard resolved == identifiers else {
            throw AppleContactsProviderError.selectionUnresolved
        }
        let canonicalIdentifiers = identifiers.sorted().joined(separator: "\u{0}")
        let authentication = HMAC<SHA256>.authenticationCode(
            for: Data("contacts.subject\u{0}\(authorization.rawValue)\u{0}\(canonicalIdentifiers)".utf8),
            using: handleKey
        )
        let fingerprint = Data(authentication).map { String(format: "%02x", $0) }.joined()
        return AppleContactsSubject(
            fingerprint: fingerprint,
            permissionClass: authorization.rawValue,
            resolvedHandles: handles.sorted()
        )
    }

    private func budgetedView(
        sourceHandle: String,
        observedAtUnixMilliseconds: Int64,
        expiresAtUnixMilliseconds: Int64,
        requestedCoverageComplete: Bool,
        identities: [AppleContactIdentity]
    ) -> AppleContactsPeopleView {
        var boundedIdentities = identities
        var contentWasReduced = false
        while true {
            let view = AppleContactsPeopleView(
                sourceHandle: sourceHandle,
                observedAtUnixMilliseconds: observedAtUnixMilliseconds,
                expiresAtUnixMilliseconds: expiresAtUnixMilliseconds,
                coverageComplete: requestedCoverageComplete && !contentWasReduced,
                identities: boundedIdentities
            )
            if Self.serializedSize(of: view) <= Self.maximumSerializedViewBytes {
                return view
            }
            contentWasReduced = true
            if let index = boundedIdentities.lastIndex(where: { !$0.aliases.isEmpty }) {
                let identity = boundedIdentities[index]
                boundedIdentities[index] = AppleContactIdentity(
                    replacingAliasesOf: identity,
                    with: Array(identity.aliases.dropLast())
                )
            } else if !boundedIdentities.isEmpty {
                boundedIdentities.removeLast()
            }
        }
    }

    private static func serializedSize(of view: AppleContactsPeopleView) -> Int {
        (try? JSONEncoder().encode(view).count) ?? .max
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
        nativeIdentifiersByHandle[identityHandle] = record.identifier
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
