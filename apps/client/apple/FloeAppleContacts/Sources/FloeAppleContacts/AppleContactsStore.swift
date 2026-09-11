import Contacts
import Foundation

struct AppleContactRecord: Equatable, Sendable {
    let identifier: String
    let displayName: String
    let nickname: String
    let emailAddresses: [String]
    let phoneNumbers: [String]
}

struct AppleContactBatch: Equatable, Sendable {
    let records: [AppleContactRecord]
    let coverageComplete: Bool
}

protocol AppleContactsStore: AnyObject {
    func authorizationState() -> AppleContactsAuthorizationState
    func requestAuthorization() async throws -> Bool
    func fetchContacts(limit: Int) throws -> AppleContactBatch
}

final class SystemAppleContactsStore: AppleContactsStore {
    private let store: CNContactStore

    init(store: CNContactStore = CNContactStore()) {
        self.store = store
    }

    func authorizationState() -> AppleContactsAuthorizationState {
        let status = CNContactStore.authorizationStatus(for: .contacts)
        switch status {
        case .notDetermined:
            return .notDetermined
        case .restricted:
            return .restricted
        case .denied:
            return .denied
        case .authorized:
            return .authorized
#if os(iOS)
        case .limited:
            return .limited
#endif
        @unknown default:
            return .restricted
        }
    }

    func requestAuthorization() async throws -> Bool {
        try await withCheckedThrowingContinuation { continuation in
            store.requestAccess(for: .contacts) { granted, error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: granted)
                }
            }
        }
    }

    func fetchContacts(limit: Int) throws -> AppleContactBatch {
        let formatterDescriptor = CNContactFormatter.descriptorForRequiredKeys(for: .fullName)
        let keys: [CNKeyDescriptor] = [
            CNContactIdentifierKey as CNKeyDescriptor,
            formatterDescriptor,
            CNContactNicknameKey as CNKeyDescriptor,
            CNContactOrganizationNameKey as CNKeyDescriptor,
            CNContactEmailAddressesKey as CNKeyDescriptor,
            CNContactPhoneNumbersKey as CNKeyDescriptor,
        ]
        let request = CNContactFetchRequest(keysToFetch: keys)
        request.sortOrder = .userDefault
        request.unifyResults = true
        var records: [AppleContactRecord] = []
        var coverageComplete = true
        try store.enumerateContacts(with: request) { contact, stop in
            guard records.count < limit else {
                coverageComplete = false
                stop.pointee = true
                return
            }
            let formattedName = CNContactFormatter.string(from: contact, style: .fullName) ?? ""
            let displayName = formattedName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? contact.organizationName
                : formattedName
            records.append(
                AppleContactRecord(
                    identifier: contact.identifier,
                    displayName: displayName,
                    nickname: contact.nickname,
                    emailAddresses: contact.emailAddresses.map { $0.value as String },
                    phoneNumbers: contact.phoneNumbers.map { $0.value.stringValue }
                )
            )
        }
        return AppleContactBatch(records: records, coverageComplete: coverageComplete)
    }
}
