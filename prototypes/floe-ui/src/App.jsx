import { useCallback, useRef, useState } from 'react';
import { ToastViewport } from './components/ui/ToastViewport.jsx';
import { GlobalSidebar } from './components/shell/GlobalSidebar.jsx';
import { TodayScreen } from './components/reference/TodayScreen.jsx';
import { TaskDetail } from './components/tasks/TaskDetail.jsx';
import { NotesCollection } from './components/notes/NotesCollection.jsx';
import { CalendarScreen } from './CalendarScreen.jsx';
import { ProgressScreen } from './ProgressScreen.jsx';

export function App() {
  const [screen, setScreen] = useState('today');
  const [toasts, setToasts] = useState([]);
  const [heights, setHeights] = useState({});
  const toastId = useRef(0);
  const notify = useCallback((title, description, tone = 'success') => {
    const id = ++toastId.current;
    setToasts((current) => [...current.slice(-2), { id, title, description, tone }]);
    setHeights((current) => Object.fromEntries(Object.entries(current).filter(([key]) => Number(key) > id - 3)));
  }, []);
  const dismissToast = useCallback((id) => setToasts((current) => current.filter((toast) => toast.id !== id)), []);
  const measureToast = useCallback((id, height) => {
    setHeights((current) => current[id] === height ? current : { ...current, [id]: height });
  }, []);

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
              notify={notify}
            />
          </div>
          {screen === 'reference' && <TodayScreen onNavigate={setScreen} />}
          {screen === 'tasks' && <TaskDetail />}
          {screen === 'notes' && <NotesCollection />}
          {screen === 'progress' && <ProgressScreen />}
        </div>
      </div>
      <ToastViewport toasts={toasts} onDismiss={dismissToast} heights={heights} onMeasure={measureToast} />
    </main>
  );
}
