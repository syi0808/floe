/// Result of an owner-wide lookup for an immutable command id.
///
/// `Occupied` means a durable command admission exists in another Expert
/// command family. Callers must retain the id because it cannot be reassigned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertCommandLookup<T> {
    Absent,
    Occupied,
    Existing(T),
}
