export function layoutTimedEvents(events) {
  const sorted = [...events].sort(
    (first, second) =>
      first.startMinutes - second.startMinutes ||
      second.endMinutes - first.endMinutes ||
      first.id.localeCompare(second.id),
  );
  const result = [];
  let group = [];
  let columnEnds = [];
  let groupEnd = -Infinity;

  function finishGroup() {
    result.push(...group.map((event) => ({ ...event, columns: columnEnds.length })));
    group = [];
    columnEnds = [];
  }

  for (const event of sorted) {
    if (event.startMinutes >= groupEnd) finishGroup();
    let column = columnEnds.findIndex((end) => end <= event.startMinutes);
    if (column === -1) column = columnEnds.length;
    columnEnds[column] = event.endMinutes;
    group.push({ ...event, column });
    groupEnd = Math.max(groupEnd, event.endMinutes);
  }
  finishGroup();
  return result;
}
