export function nextEnabled(items, current, direction) {
  for (let offset = 1; offset <= items.length; offset += 1) {
    const index = (current + direction * offset + items.length) % items.length;
    if (!items[index].disabled) return index;
  }
  return -1;
}

export function matchingOption(items, query, current) {
  const normalized = query.toLocaleLowerCase();
  for (let offset = 1; offset <= items.length; offset += 1) {
    const index = (current + offset + items.length) % items.length;
    if (!items[index].disabled && items[index].label.toLocaleLowerCase().startsWith(normalized)) return index;
  }
  return current;
}
