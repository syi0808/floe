import Foundation
@testable import FloeAppleContacts
import XCTest

final class AppleContactsProviderTests: XCTestCase {
    private let secret = Data(repeating: 7, count: 32)
    private let instant = Date(timeIntervalSince1970: 1)

    func testConnectionSnapshotsCoverEveryAuthorizationState() throws {
        let expected: [(AppleContactsAuthorizationState, AppleContactsAccessScope, Bool, Bool)] = [
            (.notDetermined, .none, true, false),
            (.limited, .selected, false, true),
            (.authorized, .full, false, true),
            (.denied, .none, false, false),
            (.restricted, .none, false, false),
        ]
        for (state, scope, canRequest, canRead) in expected {
            let provider = try makeProvider(store: MockContactsStore(state: state))
            let snapshot = provider.connectionSnapshot()
            XCTAssertEqual(snapshot.accessScope, scope)
            XCTAssertEqual(snapshot.canRequestAuthorization, canRequest)
            XCTAssertEqual(snapshot.canRead, canRead)
        }
    }

    func testRequestAuthorizationTransitionsFromNotDetermined() async throws {
        let store = MockContactsStore(state: .notDetermined, requestedState: .limited)
        let provider = try makeProvider(store: store)
        let snapshot = try await provider.requestAuthorization()
        XCTAssertEqual(snapshot.authorization, .limited)
        XCTAssertEqual(store.requestCount, 1)
    }

    func testDeniedRestrictedAndNotDeterminedCannotRead() throws {
        for state in [
            AppleContactsAuthorizationState.notDetermined,
            .denied,
            .restricted,
        ] {
            let provider = try makeProvider(store: MockContactsStore(state: state))
            XCTAssertThrowsError(try provider.readPeopleView()) { error in
                XCTAssertEqual(error as? AppleContactsProviderError, .permissionRequired(state))
            }
        }
    }

    func testLimitedAndAuthorizedCanReadBoundedOpaqueProjection() throws {
        for state in [AppleContactsAuthorizationState.limited, .authorized] {
            let records = (0..<70).map { index in
                AppleContactRecord(
                    identifier: "raw-id-\(index)",
                    displayName: "Person \(index)",
                    nickname: index == 0 ? "Friend" : "",
                    emailAddresses: index == 0 ? [" ALEX@example.com "] : [],
                    phoneNumbers: index == 0 ? ["+82 (10) 1234-5678"] : []
                )
            }
            let store = MockContactsStore(state: state, records: records)
            let provider = try makeProvider(store: store)
            let view = try provider.readPeopleView(limit: 64)
            XCTAssertEqual(view.identities.count, 64)
            XCTAssertFalse(view.coverageComplete)
            XCTAssertFalse(view.sourceHandle.contains("raw-id"))
            XCTAssertTrue(view.identities.allSatisfy { !$0.identityHandle.contains("raw-id") })
            let person = try XCTUnwrap(view.identities.first { $0.displayName == "Person 0" })
            XCTAssertEqual(
                person.aliases,
                ["name:Friend", "email:alex@example.com", "phone:+821012345678"]
            )
        }
    }

    func testExplicitSelectionReturnsOnlyRequestedOpaqueIdentity() throws {
        let store = MockContactsStore(
            state: .authorized,
            records: [
                record(identifier: "one", name: "One"),
                record(identifier: "two", name: "Two"),
            ]
        )
        let provider = try makeProvider(store: store)
        let all = try provider.readPeopleView()
        let selectedHandle = try XCTUnwrap(all.identities.first { $0.displayName == "Two" }?.identityHandle)
        let selected = try provider.readPeopleView(selection: .identityHandles([selectedHandle]))
        XCTAssertEqual(selected.identities.map(\.displayName), ["Two"])
        XCTAssertTrue(selected.coverageComplete)
    }

    func testMissingSelectedIdentityMarksCoverageIncomplete() throws {
        let provider = try makeProvider(
            store: MockContactsStore(state: .authorized, records: [record(identifier: "one", name: "One")])
        )
        let selected = try provider.readPeopleView(selection: .identityHandles(["person.identity:missing"]))
        XCTAssertTrue(selected.identities.isEmpty)
        XCTAssertFalse(selected.coverageComplete)
    }

    func testEncodedViewMatchesStrictFixtureShapeAndContainsNoForbiddenFields() throws {
        let fixtureURL = try XCTUnwrap(Bundle.module.url(forResource: "people_view_shape", withExtension: "json"))
        let fixture = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: fixtureURL)) as? [String: Any]
        )
        let provider = try makeProvider(
            store: MockContactsStore(state: .authorized, records: [record(identifier: "one", name: "One")])
        )
        let encoded = try JSONEncoder().encode(provider.readPeopleView())
        let actual = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        XCTAssertEqual(Set(actual.keys), Set(fixture.keys))
        let fixtureIdentity = try XCTUnwrap((fixture["identities"] as? [[String: Any]])?.first)
        let actualIdentity = try XCTUnwrap((actual["identities"] as? [[String: Any]])?.first)
        XCTAssertEqual(Set(actualIdentity.keys), Set(fixtureIdentity.keys))
        let serialized = String(decoding: encoded, as: UTF8.self)
        for forbidden in ["note", "postal", "birthday", "raw-id", "authority", "write"] {
            XCTAssertFalse(serialized.contains(forbidden))
        }
    }

    func testRejectsInvalidLimitsSelectionsAndShortSecrets() throws {
        XCTAssertThrowsError(
            try AppleContactsProvider(store: MockContactsStore(state: .authorized), handleSecret: Data())
        )
        let provider = try makeProvider(store: MockContactsStore(state: .authorized))
        XCTAssertThrowsError(try provider.readPeopleView(limit: 0))
        XCTAssertThrowsError(try provider.readPeopleView(limit: 65))
        XCTAssertThrowsError(try provider.readPeopleView(selection: .identityHandles([])))
        XCTAssertThrowsError(
            try provider.readPeopleView(selection: .identityHandles([String(repeating: "x", count: 129)]))
        )
    }

    private func makeProvider(store: MockContactsStore) throws -> AppleContactsProvider {
        try AppleContactsProvider(store: store, handleSecret: secret, now: { self.instant })
    }

    private func record(identifier: String, name: String) -> AppleContactRecord {
        AppleContactRecord(
            identifier: identifier,
            displayName: name,
            nickname: "",
            emailAddresses: [],
            phoneNumbers: []
        )
    }
}

private final class MockContactsStore: AppleContactsStore {
    private(set) var state: AppleContactsAuthorizationState
    private(set) var requestCount = 0
    let requestedState: AppleContactsAuthorizationState
    let records: [AppleContactRecord]

    init(
        state: AppleContactsAuthorizationState,
        requestedState: AppleContactsAuthorizationState = .authorized,
        records: [AppleContactRecord] = []
    ) {
        self.state = state
        self.requestedState = requestedState
        self.records = records
    }

    func authorizationState() -> AppleContactsAuthorizationState {
        state
    }

    func requestAuthorization() async throws -> Bool {
        requestCount += 1
        state = requestedState
        return state == .authorized || state == .limited
    }

    func fetchContacts(limit: Int) throws -> AppleContactBatch {
        AppleContactBatch(
            records: Array(records.prefix(limit)),
            coverageComplete: records.count <= limit
        )
    }
}
