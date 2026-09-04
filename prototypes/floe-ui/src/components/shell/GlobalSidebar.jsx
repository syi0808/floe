import { BarChart3, CalendarDays, ListTodo, Link2, NotebookPen } from 'lucide-react';
import { SQUIRCLE_RADIUS, SquircleButton, SquircleSurface } from '../../primitives.jsx';
import { mascotUrl } from '../../assets.js';

const navigationItems = [
  { id: 'today', label: 'Today', icon: CalendarDays },
  { id: 'tasks', label: 'Tasks', icon: ListTodo },
  { id: 'notes', label: 'Notes', icon: NotebookPen },
  { id: 'connections', label: 'Connect', icon: Link2 },
  { id: 'progress', label: 'Progress', icon: BarChart3 },
];

export function GlobalSidebar({ screen, onNavigate }) {
  return (
    <SquircleSurface
      radius={SQUIRCLE_RADIUS.card}
      className="sidebar-shell"
      contentClassName="global-sidebar"
      as="aside"
    >
      <button
        className="brand"
        type="button"
        aria-label="Floe home"
        title="Floe"
        onClick={() => onNavigate('today')}
      >
        <img src={mascotUrl} alt="" />
      </button>

      <nav className="global-nav" aria-label="Primary navigation">
        {navigationItems.map((item) => {
          const Icon = item.icon;

          return (
            <SquircleButton
              key={item.id}
              radius={SQUIRCLE_RADIUS.control}
              className={screen === item.id ? 'nav-link active' : 'nav-link'}
              aria-current={screen === item.id ? 'page' : undefined}
              aria-label={item.label}
              title={item.label}
              onClick={() => onNavigate(item.id)}
            >
              <Icon size={19} strokeWidth={1.8} aria-hidden="true" />
            </SquircleButton>
          );
        })}
      </nav>
    </SquircleSurface>
  );
}
