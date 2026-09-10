# ADR 0022: Use generalizable judgment principles in agent prompts

- **Status:** accepted
- **Date:** 2026-09-10
- **Amends:** ADR 0017 prompt assembly and ADR 0018 Manager/Expert delegation

## Context

Manager and Expert evaluations exposed three related response failures: progress text
described internal execution rather than the user's task, summaries repeated
mechanical range details and internal epistemic labels, and a later request was
answered from evidence that did not cover its wider time range.

Encoding each observed failure as a narrow prompt rule would make the prompts a
growing catalog of examples. It would also encourage literal pattern matching, such
as optimizing only the word "today," rather than improving judgment for other time
ranges, domains and capability arrangements. Conversely, prescribing a fixed
reasoning procedure or mandatory response outline would constrain agents beyond what
the task requires.

## Decision

Product prompts use common, reusable judgment principles before case-specific
examples:

1. **Goal relevance:** frame progress and results around the assignment or user goal.
   Discuss execution mechanics when they materially affect the recipient's
   understanding, choices or next action.
2. **Information value:** select context and precision according to their usefulness
   for understanding, deciding or acting. Compress details that are redundant,
   mechanically implied or immaterial.
3. **Evidence fitness:** ground conclusions in evidence that adequately covers the
   current request's subject, scope and time. Reassess that coverage when the request
   changes and seek appropriate evidence when it is insufficient.
4. **Natural synthesis:** preserve meaningful distinctions and uncertainty while
   translating internal schemas, labels and execution concepts into task-appropriate
   language. Internal epistemic categories do not define a required response format.
5. **Material limitations:** surface evidence or capability limits when they affect
   completeness, confidence or a decision, rather than reporting every internal
   boundary by default.

Examples may illustrate these principles but do not replace them. Examples should be
clearly non-exhaustive and should vary when evaluation shows that a model is copying a
surface pattern instead of applying the underlying judgment.

Absolute prohibitions remain appropriate for genuine integrity, safety, privacy and
authority boundaries, such as fabricating evidence, following retrieved instructions,
revealing hidden reasoning or mutating without authority. Preferences about wording,
detail, delegation and presentation should normally state the desired judgment and
its purpose rather than use unconditional prohibitions.

Role prompts refine the common principles without fixing a workflow:

- The Manager assesses whether available evidence fits each new request, chooses
  suitable evidence or expertise and owns a user-centered synthesis.
- Experts return domain-natural, Manager-ready findings and surface only limitations
  that materially affect their result.
- Capability guidance treats progress text as recipient-relevant status, not as a
  trace of tools, agents or infrastructure.

## Consequences

- Prompt reviews evaluate whether guidance transfers across equivalent scenarios,
  not merely whether one reported phrase disappears.
- Regression suites should vary dates, ranges, follow-up wording and available
  capabilities while asserting evidence acquisition and response qualities.
- Prompt improvements alone cannot create unavailable evidence. Runtime grants and
  source adapters must support the scope an agent is expected to obtain; otherwise
  the agent should preserve the material limitation rather than imply completeness.
- Prompt component revisions change whenever their stable product instruction text
  changes so manifests retain meaningful prompt identity.

## References

- [Agent context assembly](0017-agent-context-assembly.md)
- [Manager/Expert delegation](0018-manager-expert-a2a-delegation.md)
- [Agent context and Schedule Expert generalization](../validation/s4-agent-context-generalization.md)
