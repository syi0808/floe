export function dstFixture(mode) {
  if (!['spring', 'fall'].includes(mode)) return null;
  const spring = mode === 'spring';
  const start = new Date(spring ? '2026-03-08T08:00:00Z' : '2026-11-01T07:00:00Z');
  const hours = spring ? 23 : 25;
  const formatter = new Intl.DateTimeFormat('en-GB', { timeZone: 'America/Los_Angeles', hour: '2-digit', minute: '2-digit', timeZoneName: 'short', hourCycle: 'h23' });
  const makeEvent = (id, startMinutes, endMinutes) => ({
    id, calendarId: 'work', title: id === 'dst-first' ? 'Across the clock change' : 'The repeated hour',
    startMinutes, endMinutes, time: `${formatter.format(new Date(+start + startMinutes * 60000))} – ${formatter.format(new Date(+start + endMinutes * 60000))}`,
    detail: `${endMinutes - startMinutes} real minutes · clock-change fixture`, timezone: 'America/Los_Angeles',
    original: 'Local wall time with offset shown above',
  });
  return { date: spring ? '2026-03-08' : '2026-11-01', minutes: hours * 60,
    labels: Array.from({ length: hours + 1 }, (_, index) => index === hours ? '24:00' : formatter.format(new Date(+start + index * 3600000))),
    events: spring ? [makeEvent('dst-first', 90, 150)] : [makeEvent('dst-first', 90, 105), makeEvent('dst-second', 150, 165)],
  };
}
