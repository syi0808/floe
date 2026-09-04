import { useState } from 'react';
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
      <div className={`app-window ${screen === 'today' ? 'calendar-window' : ''}`}>
        <GlobalSidebar
          screen={screen === 'calendar-connection' ? 'connections' : screen}
          onNavigate={setScreen}
        />
        <div className="screen-region">
          <div hidden={!['today', 'connections', 'calendar-connection'].includes(screen)}>
            <CalendarScreen
              page={['connections', 'calendar-connection'].includes(screen) ? screen : 'day'}
              onNavigate={setScreen}
            />
          </div>
          {screen === 'reference' && <TodayScreen onNavigate={setScreen} />}
          {screen === 'tasks' && <TaskDetail />}
          {screen === 'notes' && <NotesCollection />}
          {screen === 'progress' && <ProgressScreen />}
        </div>
      </div>
    </main>
  );
}
