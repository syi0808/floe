export const summary = [
  { label: 'Personal Day slice', value: 78, detail: '75–80%', tone: 'violet' },
  { label: 'MVP engineering', value: 58, detail: '55–60%', tone: 'blue' },
  { label: 'Phase 1 · Personal Day', value: 40, detail: '35–45%', tone: 'mint' },
];

export const mvpAreas = [
  {
    label: 'Day Canvas',
    value: 70,
    note: 'Now/Next · unified projection',
    tone: 'violet',
  },
  {
    label: 'Universal Capture',
    value: 60,
    note: 'Typed capture · provenance',
    tone: 'blue',
  },
  {
    label: 'Local Day Store',
    value: 83,
    note: 'Turso · CRUD · persistence',
    tone: 'mint',
  },
  {
    label: 'Minimal Assistant',
    value: 0,
    note: 'Manager · proposals · policy',
    tone: 'amber',
  },
];

export const delivered = [
  'Rust domain + typed operations',
  'Deterministic Day snapshot',
  'Embedded Turso persistence',
  'Flutter ↔ Rust JSON/C ABI',
  'Typed capture + classification',
  'Task completion + deletion',
];

export const remaining = [
  'Event, task, note editing UI',
  'Typed conflict recovery',
  'Read-only calendar integration',
  'Dense-day folding',
  'Two-week Day Canvas dogfood',
];

export const phases = [
  { phase: '0', label: 'Architecture PoCs', value: 13, status: 'active' },
  { phase: '1', label: 'Personal Day', value: 40, status: 'active' },
  { phase: '2', label: 'Connected Floe', value: 0, status: 'queued' },
  { phase: '3', label: 'Personal Memory', value: 0, status: 'queued' },
  { phase: '3.5', label: 'Expert Ecosystem', value: 0, status: 'queued' },
  { phase: '4', label: 'Cross-device', value: 0, status: 'queued' },
  { phase: '5', label: 'Ambient Floe', value: 0, status: 'queued' },
  { phase: '6', label: 'Hosted / Self-host', value: 0, status: 'queued' },
];

export const priorities = [
  ['01', 'Connect editing flows', 'Complete the local Personal Day loop.'],
  ['02', 'Recover from conflicts', 'Refresh, retry, or keep user input safely.'],
  ['03', 'Import calendar context', 'Validate Calendar + Tasks + Notes together.'],
  ['04', 'Handle dense days', 'Fold and reveal without losing orientation.'],
  ['05', 'Dogfood for two weeks', 'Measure usefulness, misses, and return rate.'],
];
