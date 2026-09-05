import { ArrowRight, ShieldCheck } from 'lucide-react';
import { CalendarSurface } from './CalendarSurface.jsx';
import './calendar-action.css';

const captions = {
  pending: 'Only with your approval · prototype',
  checking: 'Checking your current schedule…',
  creating: 'Adding your event once…',
  importing: 'Created · bringing it into Today…',
  succeeded: 'Added to your day · simulated',
  rejected: 'Declined. Nothing was created.',
  conflict: 'A time conflict. Nothing was created.',
  denied: 'Calendar access needed. Nothing was created.',
  expired: 'Expired. Nothing was created.',
  unknown: 'Not confirmed. Check before trying again.',
  'looking-up': 'Checking Calendar · not creating again…',
  'read-error': 'Created in Calendar · Today read needs a retry.',
};

export function CalendarActionCard({ action, disabled, onReview }) {
  return (
    <CalendarSurface className="s3-action-card">
      <div className="s1-card-heading"><h2>A little room to focus</h2><ShieldCheck size={18} /></div>
      <p>45 quiet minutes.<br />Sep 4 · 2:45–3:30 PM</p>
      <span className="s3-action-caption" role="status">{captions[action.status]}</span>
      <button className="s1-text-link" disabled={disabled} onClick={onReview}>
        <span>{action.status === 'pending' ? 'Review suggestion' : 'View action status'}</span><ArrowRight size={15} />
      </button>
      {disabled && <small>Connect and refresh this day to review.</small>}
    </CalendarSurface>
  );
}
