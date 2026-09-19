# Personal domains

Floe keeps domain semantics separate even when they project into one assistant experience.

## Personal Timeline

Timeline answers: **what is the person doing, planning or committing to over time?**

It includes Events, Tasks, Notes, Commitments, routines and interventions. These do not become one generic record.

- Event retains interval, timezone/recurrence, attendee/availability and external-source identity semantics.
- Task retains completion, deadline, priority and recurrence semantics.
- Note retains authored content, attachments/links and creation context.
- Commitment captures promises/follow-up obligations that may not yet be an Event or Task.

External objects retain source identity, provider revision/cursor where relevant and provenance without allowing provider schemas to become Floe's domain model.

## Personal State

State answers: **what relevant condition is true now?**

State is more transient than Memory and may be recomputed from source data. Examples include next-event/free-time context, schedule density, coarse recovery/capacity, workload, attention or device/location context.

Prefer bounded derived state over raw sensitive streams when it is sufficient for the decision.

## Personal Memory

Memory is a source-backed personal context system, not a free-form preference blob or instruction store.

Keep distinct:

- user-confirmed facts;
- observations;
- inferences;
- preferences;
- episodes;
- commitments.

Evidence is not repeatedly summarized until its origin disappears. Claims and current views retain provenance. Temporal context such as `validFrom`, `validUntil`, `observedAt` and confidence should be preserved when meaningful.

Inference does not silently become fact. External Mail, Calendar descriptions, documents and notes remain untrusted data and cannot become system/product instructions.

Memory is inspectable, editable, deletable and traceable. Deletion must account for derived artifacts and provenance rather than only deleting one row.

## People and relationships

People are first-class personal context. Multiple aliases, contact identifiers, attendees or addresses can refer to one person, but uncertain identity must not be automatically merged.

Relationships are temporal rather than immutable labels. Relationship observations and inferences preserve time, evidence and uncertainty.

Future cross-Person sharing is private by default, explicit, scoped and revocable.
