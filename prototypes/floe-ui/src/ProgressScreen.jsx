import {
  ArrowUpRight,
  CheckCircle2,
  Circle,
  Flag,
  Layers3,
  ShieldCheck,
  Sparkles,
  TestTube2,
} from 'lucide-react';

import mascotUrl from '../../../assets/floe-mascot.svg?url';
import {
  SQUIRCLE_RADIUS,
  SquircleBlock,
  SquircleSurface,
} from './primitives.jsx';

const summary = [
  { label: 'Personal Day slice', value: 78, detail: '75–80%', tone: 'violet' },
  { label: 'MVP engineering', value: 58, detail: '55–60%', tone: 'blue' },
  { label: 'Phase 1 · Personal Day', value: 40, detail: '35–45%', tone: 'mint' },
];

const mvpAreas = [
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

const delivered = [
  'Rust domain + typed operations',
  'Deterministic Day snapshot',
  'Embedded Turso persistence',
  'Flutter ↔ Rust JSON/C ABI',
  'Typed capture + classification',
  'Task completion + deletion',
];

const remaining = [
  'Event, task, note editing UI',
  'Typed conflict recovery',
  'Read-only calendar integration',
  'Dense-day folding',
  'Two-week Day Canvas dogfood',
];

const phases = [
  { phase: '0', label: 'Architecture PoCs', value: 13, status: 'active' },
  { phase: '1', label: 'Personal Day', value: 40, status: 'active' },
  { phase: '2', label: 'Connected Floe', value: 0, status: 'queued' },
  { phase: '3', label: 'Personal Memory', value: 0, status: 'queued' },
  { phase: '3.5', label: 'Expert Ecosystem', value: 0, status: 'queued' },
  { phase: '4', label: 'Cross-device', value: 0, status: 'queued' },
  { phase: '5', label: 'Ambient Floe', value: 0, status: 'queued' },
  { phase: '6', label: 'Hosted / Self-host', value: 0, status: 'queued' },
];

const priorities = [
  ['01', 'Connect editing flows', 'Complete the local Personal Day loop.'],
  ['02', 'Recover from conflicts', 'Refresh, retry, or keep user input safely.'],
  ['03', 'Import calendar context', 'Validate Calendar + Tasks + Notes together.'],
  ['04', 'Handle dense days', 'Fold and reveal without losing orientation.'],
  ['05', 'Dogfood for two weeks', 'Measure usefulness, misses, and return rate.'],
];

export function ProgressScreen() {
  return (
    <div className="progress-screen">
      <header className="progress-hero">
        <div className="progress-title-block">
          <span className="progress-eyebrow">Build overview</span>
          <h1>Floe is becoming a useful day companion.</h1>
          <p>
            The technical spine works. The next milestone is turning the
            Personal Day slice into a complete, dogfood-ready product loop.
          </p>
        </div>
        <div className="progress-update">
          <span className="live-dot" aria-hidden="true" />
          <span>In progress</span>
          <time dateTime="2026-09-03">Updated Sep 3, 2026</time>
        </div>
      </header>

      <section className="progress-summary-grid" aria-label="Progress summary">
        <SquircleSurface
          radius={SQUIRCLE_RADIUS.overlay}
          className="overall-card"
          contentClassName="overall-card-inner"
        >
          <div
            className="overall-ring"
            role="img"
            aria-label="Full roadmap progress is under 10 percent"
          >
            <div>
              <strong>&lt;10</strong>
              <span>%</span>
            </div>
          </div>
          <div className="overall-copy">
            <span className="card-kicker">Full roadmap</span>
            <h2>The foundation is in place.</h2>
            <p>Focus stays narrow: finish Personal Day before expanding the surface.</p>
          </div>
          <img className="progress-mascot" src={mascotUrl} alt="" />
        </SquircleSurface>

        <div className="summary-stack">
          {summary.map((item) => (
            <SquircleSurface
              key={item.label}
              radius={SQUIRCLE_RADIUS.card}
              className="summary-card"
              contentClassName="summary-card-inner"
            >
              <div>
                <span>{item.label}</span>
                <strong>{item.detail}</strong>
              </div>
              <ProgressBar value={item.value} tone={item.tone} />
            </SquircleSurface>
          ))}
        </div>
      </section>

      <section className="progress-main-grid">
        <SquircleSurface
          radius={SQUIRCLE_RADIUS.card}
          className="dashboard-card"
          contentClassName="dashboard-card-inner"
          as="section"
        >
          <SectionHeading
            icon={Layers3}
            eyebrow="MVP areas"
            title="What works today"
            meta="Directional estimates"
          />
          <div className="area-list">
            {mvpAreas.map((area) => (
              <div className="area-row" key={area.label}>
                <div className="area-label">
                  <strong>{area.label}</strong>
                  <span>{area.note}</span>
                </div>
                <div className="area-measure">
                  <ProgressBar value={area.value} tone={area.tone} />
                  <span>{area.value}%</span>
                </div>
              </div>
            ))}
          </div>
        </SquircleSurface>

        <SquircleSurface
          radius={SQUIRCLE_RADIUS.card}
          className="dashboard-card checkpoint-card"
          contentClassName="dashboard-card-inner"
          as="section"
        >
          <SectionHeading
            icon={Flag}
            eyebrow="Current checkpoint"
            title="12 of 17 shipped"
            meta="Personal Day slice"
          />
          <div className="checkpoint-columns">
            <Checklist title="Delivered" items={delivered} complete />
            <Checklist title="Still open" items={remaining} />
          </div>
        </SquircleSurface>
      </section>

      <section className="progress-main-grid lower-grid">
        <SquircleSurface
          radius={SQUIRCLE_RADIUS.card}
          className="dashboard-card roadmap-card"
          contentClassName="dashboard-card-inner"
          as="section"
        >
          <SectionHeading
            icon={Sparkles}
            eyebrow="Roadmap"
            title="Build outward from the day"
            meta="8 phases"
          />
          <div className="phase-track">
            {phases.map((item) => (
              <div className={`phase-item ${item.status}`} key={item.phase}>
                <div className="phase-marker">
                  <span>{item.phase}</span>
                </div>
                <div className="phase-copy">
                  <strong>{item.label}</strong>
                  <span>{item.value > 0 ? `${item.value}%` : 'Not started'}</span>
                </div>
              </div>
            ))}
          </div>
          <div className="roadmap-focus">
            <div>
              <span>Now · Phase 1</span>
              <strong>Close the local Personal Day loop</strong>
            </div>
            <p>
              Connected data waits until the daily experience is useful enough
              to earn repeat use.
            </p>
          </div>
        </SquircleSurface>

        <SquircleSurface
          radius={SQUIRCLE_RADIUS.card}
          className="dashboard-card priorities-card"
          contentClassName="dashboard-card-inner"
          as="section"
        >
          <SectionHeading
            icon={ArrowUpRight}
            eyebrow="Next priorities"
            title="Path to dogfood"
            meta="Ordered"
          />
          <ol className="priority-list">
            {priorities.map(([number, title, detail]) => (
              <li key={number}>
                <span className="priority-number">{number}</span>
                <div>
                  <strong>{title}</strong>
                  <span>{detail}</span>
                </div>
              </li>
            ))}
          </ol>
        </SquircleSurface>
      </section>

      <section className="validation-strip" aria-label="Validation baseline">
        <div className="validation-title">
          <TestTube2 size={18} aria-hidden="true" />
          <span>Validation baseline</span>
        </div>
        <ValidationItem value="15" label="Rust tests" />
        <ValidationItem value="12" label="Flutter tests" />
        <ValidationItem icon={ShieldCheck} label="Clippy clean" />
        <ValidationItem icon={CheckCircle2} label="Analyzer clean" />
        <span className="source-note">Source · PROGRESS.md</span>
      </section>
    </div>
  );
}

function SectionHeading({ icon: Icon, eyebrow, title, meta }) {
  return (
    <div className="section-heading">
      <div className="section-icon" aria-hidden="true">
        <Icon size={18} strokeWidth={1.8} />
      </div>
      <div>
        <span>{eyebrow}</span>
        <h2>{title}</h2>
      </div>
      <small>{meta}</small>
    </div>
  );
}

function ProgressBar({ value, tone }) {
  return (
    <div
      className={`progress-bar ${tone}`}
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={value}
    >
      <SquircleBlock
        radius={SQUIRCLE_RADIUS.micro}
        className="progress-fill"
        style={{ width: `${Math.max(value, 2)}%` }}
      />
    </div>
  );
}

function Checklist({ title, items, complete = false }) {
  return (
    <div className="checkpoint-list">
      <h3>{title}</h3>
      <ul>
        {items.map((item) => (
          <li key={item}>
            {complete ? (
              <CheckCircle2 size={16} aria-hidden="true" />
            ) : (
              <Circle size={16} aria-hidden="true" />
            )}
            <span>{item}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function ValidationItem({ value, icon: Icon, label }) {
  return (
    <div className="validation-item">
      {value ? <strong>{value}</strong> : <Icon size={18} aria-hidden="true" />}
      <span>{label}</span>
    </div>
  );
}
