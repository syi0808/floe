use floe_agent::{AgentFailure, AgentUsage, UsageLedger};

#[test]
fn reservations_are_replaced_by_reported_usage() {
    let ledger = UsageLedger::new(100, 7, AgentUsage::default());
    let mut tokens = 1000;
    let mut cost = 100;
    let attempt = ledger.begin(&mut tokens, &mut cost).unwrap();
    assert_eq!((tokens, cost), (100, 7));
    assert_eq!(ledger.snapshot().estimated_tokens, 100);
    attempt.settle(10, 7).unwrap();
    assert_eq!(ledger.snapshot().tokens, 10);
    assert_eq!(ledger.snapshot().estimated_tokens, 0);
    assert_eq!(ledger.snapshot().cost_micros, 7);
    let attempt = ledger.begin(&mut tokens, &mut cost).unwrap();
    assert_eq!((tokens, cost), (90, 0));
    assert_eq!(attempt.settle(10, 1), Err(AgentFailure::BudgetExceeded));
    assert_eq!(ledger.snapshot().cost_micros, 8);
    assert!(ledger.begin(&mut tokens, &mut cost).is_err());
    assert_eq!(ledger.snapshot().attempts, 2);
}

#[test]
fn dropped_attempts_keep_estimates_and_release_the_single_dispatch_slot() {
    let ledger = UsageLedger::new(10_000, 0, AgentUsage::default());
    let mut tokens = 10_000;
    let mut cost = 0;
    let attempt = ledger.begin(&mut tokens, &mut cost).unwrap();
    assert!(ledger.clone().begin(&mut tokens, &mut cost).is_err());
    drop(attempt);
    assert_eq!(ledger.snapshot().estimated_tokens, 4096);
    let attempt = ledger.clone().begin(&mut tokens, &mut cost).unwrap();
    assert_eq!(tokens, 5904);
    attempt.settle(10, 0).unwrap();
    assert_eq!(ledger.snapshot().tokens, 4106);
    assert_eq!(ledger.snapshot().estimated_tokens, 4096);
    assert_eq!(ledger.snapshot().attempts, 2);
}

#[test]
fn exhausted_budget_never_records_an_attempt() {
    let ledger = UsageLedger::new(0, 0, AgentUsage::default());
    assert!(ledger.begin(&mut 100, &mut 0).is_err());
    assert_eq!(ledger.snapshot().attempts, 0);
}
