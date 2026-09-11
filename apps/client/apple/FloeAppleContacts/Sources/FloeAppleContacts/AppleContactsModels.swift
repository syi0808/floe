import Foundation

public enum AppleContactsAuthorizationState: String, Codable, CaseIterable, Sendable {
    case notDetermined = "not_determined"
    case limited
    case authorized
    case denied
    case restricted
}

public enum AppleContactsAccessScope: String, Codable, Sendable {
    case none
    case selected
    case full
}

public struct AppleContactsConnectionSnapshot: Codable, Equatable, Sendable {
    public let authorization: AppleContactsAuthorizationState
    public let accessScope: AppleContactsAccessScope
    public let canRequestAuthorization: Bool
    public let canRead: Bool

    public init(authorization: AppleContactsAuthorizationState) {
        self.authorization = authorization
        switch authorization {
        case .notDetermined:
            accessScope = .none
            canRequestAuthorization = true
            canRead = false
        case .limited:
            accessScope = .selected
            canRequestAuthorization = false
            canRead = true
        case .authorized:
            accessScope = .full
            canRequestAuthorization = false
            canRead = true
        case .denied, .restricted:
            accessScope = .none
            canRequestAuthorization = false
            canRead = false
        }
    }

    enum CodingKeys: String, CodingKey {
        case authorization
        case accessScope = "access_scope"
        case canRequestAuthorization = "can_request_authorization"
        case canRead = "can_read"
    }
}

public enum AppleContactsSelection: Sendable, Equatable {
    case allAuthorized
    case identityHandles(Set<String>)
}

public struct AppleContactIdentity: Codable, Equatable, Sendable {
    public let identityHandle: String
    public let displayName: String
    public let aliases: [String]
    public let confidenceMillis: UInt16
    public let evidenceHandles: [String]

    init(
        identityHandle: String,
        displayName: String,
        aliases: [String],
        evidenceHandle: String
    ) {
        self.identityHandle = identityHandle
        self.displayName = displayName
        self.aliases = aliases
        confidenceMillis = 1_000
        evidenceHandles = [evidenceHandle]
    }

    enum CodingKeys: String, CodingKey {
        case identityHandle = "identity_handle"
        case displayName = "display_name"
        case aliases
        case confidenceMillis = "confidence_millis"
        case evidenceHandles = "evidence_handles"
    }
}

public struct AppleContactsPeopleView: Codable, Equatable, Sendable {
    public let schemaVersion: UInt32
    public let viewID: String
    public let sourceHandle: String
    public let observedAtUnixMilliseconds: Int64
    public let expiresAtUnixMilliseconds: Int64
    public let coverageComplete: Bool
    public let identities: [AppleContactIdentity]

    init(
        sourceHandle: String,
        observedAtUnixMilliseconds: Int64,
        expiresAtUnixMilliseconds: Int64,
        coverageComplete: Bool,
        identities: [AppleContactIdentity]
    ) {
        schemaVersion = 1
        viewID = "people.identity"
        self.sourceHandle = sourceHandle
        self.observedAtUnixMilliseconds = observedAtUnixMilliseconds
        self.expiresAtUnixMilliseconds = expiresAtUnixMilliseconds
        self.coverageComplete = coverageComplete
        self.identities = identities
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case viewID = "view_id"
        case sourceHandle = "source_handle"
        case observedAtUnixMilliseconds = "observed_at_unix_ms"
        case expiresAtUnixMilliseconds = "expires_at_unix_ms"
        case coverageComplete = "coverage_complete"
        case identities
    }
}

public enum AppleContactsProviderError: Error, Equatable, Sendable {
    case invalidHandleSecret
    case invalidLimit
    case invalidSelection
    case permissionRequired(AppleContactsAuthorizationState)
    case authorizationRequestFailed
    case storeReadFailed
}
