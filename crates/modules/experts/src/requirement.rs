use floe_context_contract::SourceUnavailable;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequirementReadOutcome<Value> {
    Ready(Value),
    Unavailable(SourceUnavailable),
    NeedsUserAction,
}
