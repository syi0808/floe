# S4 managed instructions and Markdown chat

Date: 2026-09-08. Implementation and automated evidence only; S4 remains **0/14**.

## Implemented

- Moved the Manager and built-in Schedule Expert system instructions, Calendar turn
  templates and sample preset prompts out of Rust source into compile-time text
  resources under `crates/floe-agent/prompts/`.
- Added explicit formal-register and no-emoji rules to both roles. The Manager may
  use concise Markdown when it improves readability.
- Rendered assistant messages with selectable GitHub-Flavored Markdown in the
  Flutter panel. User messages remain literal selectable text.
- Kept Markdown presentational: links have no navigation callback and image syntax
  renders only alt text, preventing model-authored Markdown from fetching a remote or
  local resource.
- Added focused Rust assertions for both instruction baselines and bounded template
  expansion, plus a Flutter widget test covering emphasis, lists, selection and the
  safe image override.

## Validation boundary

Automated tests verify embedded instruction contents and renderer wiring. They do not
prove that every model follows tone instructions, evaluate prompt-injection resistance
or validate a live Foundation Models conversation. Live model/source/privacy gates and
the complete S4 acceptance matrix remain unchanged.
