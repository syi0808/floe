import { useState } from 'react';
import { Settings } from 'lucide-react';
import { SQUIRCLE_RADIUS, SquircleButton, SquircleSurface } from './primitives.jsx';
import { GlobalSidebar } from './components/shell/GlobalSidebar.jsx';
import { TodayScreen } from './components/reference/TodayScreen.jsx';
import { TaskDetail } from './components/tasks/TaskDetail.jsx';
import { NotesCollection } from './components/notes/NotesCollection.jsx';
import { CalendarScreen } from './CalendarScreen.jsx';
import { ProgressScreen } from './ProgressScreen.jsx';

export function App() {
  const [screen, setScreen] = useState('today');

  return (
    <main className="prototype-stage">
      <SquircleSurface
        radius={SQUIRCLE_RADIUS.frame}
        className="app-frame"
        contentClassName="app-window"
      >
        <GlobalSidebar screen={screen} onNavigate={setScreen} />
        <SquircleButton
          className="settings-action"
          aria-label="Settings"
          title="Settings"
          onClick={() => setScreen('connections')}
        >
          <Settings size={20} strokeWidth={1.8} />
        </SquircleButton>
        <div className="screen-region">
          <div hidden={!['today', 'connections'].includes(screen)}>
            <CalendarScreen
              page={screen === 'connections' ? 'connections' : 'day'}
              onNavigate={setScreen}
            />
          </div>
          {screen === 'reference' && <TodayScreen onNavigate={setScreen} />}
          {screen === 'tasks' && <TaskDetail />}
          {screen === 'notes' && <NotesCollection />}
          {screen === 'progress' && <ProgressScreen />}
        </div>
      </SquircleSurface>
    </main>
  );
}
