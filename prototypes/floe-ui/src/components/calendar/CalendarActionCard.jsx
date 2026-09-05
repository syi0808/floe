import { ArrowRight, ShieldCheck } from 'lucide-react';
import { CalendarSurface } from './CalendarSurface.jsx';
import './calendar-action.css';

export function CalendarActionCard({ action, disabled, onReview }) {
  return (
    <CalendarSurface className="s3-action-card">
      <div className="s1-card-heading"><h2>A little room to focus</h2><ShieldCheck size={18} /></div>
      <p>45 quiet minutes.<br />Sep 4 · 2:45–3:30 PM</p>
      <span className="s3-action-caption">{action.status === 'succeeded' ? 'Added to your day · simulated' : action.status === 'rejected' ? 'Declined. Nothing was created.' : 'Only with your approval · prototype'}</span>
      <button className="s1-text-link" disabled={disabled} onClick={onReview}>
        <span>{action.status === 'pending' ? 'Review suggestion' : 'View action status'}</span><ArrowRight size={15} />
      </button>
      {disabled && <small>Connect and refresh this day to review.</small>}
    </CalendarSurface>
  );
}
