public protocol Transform<Input, Output>: Sendable {
    associatedtype Input: Sendable
    associatedtype Output: Sendable

    func transform(_ input: Input) async throws -> Output
}
