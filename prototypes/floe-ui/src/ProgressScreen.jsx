import {
  summary,
  mvpAreas,
  delivered,
  remaining,
  phases,
  priorities,
} from './components/progress/progress-fixtures.js';
import {
  ArrowUpRight,
  CheckCircle2,
  Flag,
  Layers3,
  ShieldCheck,
  Sparkles,
  TestTube2,
} from 'lucide-react';
import { SQUIRCLE_RADIUS, SquircleSurface } from './primitives.jsx';
import { mascotUrl } from './assets.js';
import { SectionHeading } from './components/progress/SectionHeading.jsx';
import { ProgressBar } from './components/progress/ProgressBar.jsx';
import { Checklist } from './components/progress/Checklist.jsx';
import { ValidationItem } from './components/progress/ValidationItem.jsx';

export function ProgressScreen() {
  return (
    <div className="progress-screen">
      <header className="progress-hero">
        <div className="progress-title-block">
          <span className="progress-eyebrow">Build overview</span>
          <h1>Floe is becoming a useful day companion.</h1>
          <p>
            The technical spine works. The next milestone is turning the Personal Day slice into a
            complete, dogfood-ready product loop.
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
              Connected data waits until the daily experience is useful enough to earn repeat use.
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
