# ADR 0014: Include bounded real-world sources in S4

- **Date:** 2026-09-07
- **Status:** accepted
- **Amends:** [ADR 0013](0013-conversational-agent-learning-and-voice-sequence.md)

## Context

A conversational Agent/Expert runtime validated only against Calendar and fixtures
would leave a major extensibility risk untested: whether heterogeneous server and
device sources can expose useful, least-privilege Views without leaking raw data or
coupling the Agent to provider APIs.

Gmail, Contacts, location/ETA/weather, Screen Time and Apple Health exercise
different boundaries:

- Gmail is an OAuth, remote-API, indexed/on-demand content source executed through
  Floe's local Go boundary in S4.
- Contacts is limited-access identity evidence rather than Personal Memory.
- location, ETA and weather are ephemeral feasibility inputs with distinct privacy
  and attribution requirements.
- Screen Time is an entitlement- and region-constrained Apple device-context API.
- Apple Health is highly sensitive, device-native and unavailable for data access
  on macOS; only derived state should reach the Agent.

## Decision

- Expand S4 into **Conversational Connected Agent and Expert Foundation**.
- Keep Calendar as the action loop and add six connector acceptance groups:
  common connection/capability behavior, Gmail, Contacts, next-event feasibility,
  Screen Time feasibility, and Apple Health derived state.
- Run the Gmail connector in the existing local Go boundary for S4. This does not
  implement accounts, hosted sync or the S8 server.
- Validate Screen Time and Apple Health on a signed physical iOS/iPadOS device.
  Their local connector tests do not imply cross-device delivery to the macOS app.
- Expose provider-neutral `MailView`, `PeopleView`, `NextEventFeasibilityView`,
  `AttentionStateView` and `HealthStateView` to Experts. The Agent never receives
  OAuth tokens, native API objects, raw location history, Screen Time records or
  raw HealthKit samples.
- Use only public Apple APIs and entitlements. Do not read private Screen Time
  databases or treat a development entitlement as distribution approval.
- Record Screen Time as an explicit availability gate. If public API, entitlement
  or region constraints cannot provide the required derived signal, S4 documents
  the unsupported capability and removes it from required product scope rather
  than shipping a private workaround.

## Connector scope

### Gmail

- `gmail.readonly`-equivalent least privilege for S4;
- message/thread IDs and search index, body read only on demand;
- initial bounded import plus restart-safe incremental cursor/history handling;
- disconnect/revoke removes future access without inventing deletion semantics;
- external mail content remains untrusted evidence.

### Screen Time

- individual authorization on a physical supported Apple device;
- privacy-preserving aggregate/threshold signal only;
- explicit entitlement, authorization, OS and region availability states;
- no app restriction/shield mutation in S4.

### Apple Health

- physical iPhone/iPad HealthKit availability and per-type read authorization;
- a minimal sleep/activity/recovery-oriented local derivation;
- raw samples remain inside the device health boundary;
- Agent/Expert receives freshness, confidence, provenance class and coarse derived
  state, not diagnosis or raw metrics by default.

### Contacts and next-event feasibility

- Contacts uses limited access where possible and stores identity references, not
  an address-book copy or contact notes.
- current location starts with When In Use/reduced accuracy and is not retained as
  movement history.
- Map ETA is requested only for a visible next-event question and cached briefly.
- Weather is bounded to the event window and preserves required attribution.

## Consequences

- S4 is larger, so connector work is organized as independently demonstrable S4-C
  criteria after the core Agent contract rather than hidden inside one acceptance
  item.
- The common registry and View contract are tested against server-native and
  device-native implementations before Memory and voice depend on them.
- Apple connector evidence requires a physical supported device and signing; CI
  uses contract fixtures and cannot claim live acceptance.
- Cross-device convergence remains S8. S4 must label iOS-only results rather than
  imply that macOS can read Apple Health or Screen Time directly.

## References

- [Vertical Slice Delivery](../planning/08-engineering/vertical-slice-delivery.md)
- [Initial Connector Set](../planning/05-integrations/initial-connector-set.md)
- [Assistant Context Portfolio](../planning/05-integrations/assistant-context-portfolio.md)
- [Gmail message listing](https://developers.google.com/workspace/gmail/api/guides/list-messages)
- [Screen Time frameworks](https://developer.apple.com/documentation/ScreenTimeAPIDocumentation)
- [Family Controls entitlement](https://developer.apple.com/documentation/xcode/configuring-family-controls)
- [FamilyActivityData availability](https://developer.apple.com/documentation/familycontrols/familyactivitydata)
- [HealthKit platform availability](https://developer.apple.com/documentation/healthkit/about-the-healthkit-framework)
- [Apple Contacts access](https://developer.apple.com/documentation/contacts/accessing-the-contact-store)
- [Core Location authorization](https://developer.apple.com/documentation/corelocation/requesting-authorization-to-use-location-services)
- [MapKit ETA](https://developer.apple.com/documentation/mapkit/mkdirections)
- [WeatherKit](https://developer.apple.com/weatherkit/)
