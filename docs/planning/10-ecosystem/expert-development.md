# Expert Development Experience

> Status: Direction

## Two Levels of Extension

Floe should support both non-programmer customization and developer extensions.

---

# Level 1 — Create an Expert

A user may eventually describe a desired assistant behavior:

```text
"채용 관련 메일을 보고 다음 액션을 챙겨주는 비서를 만들어줘."
```

Floe can generate a **Declarative Expert draft**.

The generated artifact should still be inspectable:

- triggers
- permissions
- rules
- model usage
- outputs

Natural-language creation is an authoring experience, not a hidden unrestricted agent.

---

# Level 2 — Expert SDK

Developers can create packages using:

- manifest/schema
- declarative pipeline
- optional Wasm component
- local testing harness

Potential CLI:

```text
floe expert init
floe expert dev
floe expert test
floe expert pack
floe expert inspect-permissions
```

---

# Testing

Expert SDK should support deterministic fixtures.

Example:

```text
Fixture
├─ fake TimelineView
├─ fake MailView
├─ fake MemoryView
└─ trigger
       ↓
Expert
       ↓
Expected ActionProposal / Insight
```

Third-party Experts should be testable without the developer's personal Floe database.

---

# Simulation

A development mode can replay anonymized/synthetic events.

```text
calendar.changed
mail.received
state.condition.changed
```

This is valuable for both correctness and marketplace review.

---

# Publishing

Potential flow:

```text
floe expert pack
   ↓
permission/static analysis
   ↓
sign
   ↓
publish
```

Marketplace review can combine:

- manifest validation
- package signature
- static permission analysis
- Wasm import inspection
- resource-budget tests
- automated fixture tests
- human review for sensitive categories
