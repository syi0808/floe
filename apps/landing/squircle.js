/** One continuous-corner surface primitive for the static page.
 * Uses a sampled fourth-order superellipse, shared by every semantic radius.
 * Keeping the decorative SVG behind the element preserves native hit targets,
 * selection, and an unclipped external keyboard focus outline.
 */
const NS = 'http://www.w3.org/2000/svg';

function shapePath(width, height, radius) {
  const inset = 0.5;
  const r = Math.max(0, Math.min(radius, (width - 1) / 2, (height - 1) / 2));
  const corners = [
    [width - r - inset, r + inset, -Math.PI / 2],
    [width - r - inset, height - r - inset, 0],
    [r + inset, height - r - inset, Math.PI / 2],
    [r + inset, r + inset, Math.PI],
  ];
  const points = [];
  for (const [cx, cy, start] of corners) {
    for (let step = 0; step <= 32; step++) {
      const angle = start + (step / 32) * Math.PI / 2;
      const cos = Math.cos(angle);
      const sin = Math.sin(angle);
      const x = cx + r * Math.sign(cos) * Math.sqrt(Math.abs(cos));
      const y = cy + r * Math.sign(sin) * Math.sqrt(Math.abs(sin));
      points.push(`${x.toFixed(2)},${y.toFixed(2)}`);
    }
  }
  return `M${points.join(' L')} Z`;
}

export function mountSquircles(root = document) {
  const observer = new ResizeObserver(entries => {
    for (const { target } of entries) {
      const { width, height } = target.getBoundingClientRect();
      if (!width || !height) continue;
      const svg = target.querySelector(':scope > .squircle-shape');
      const radius = parseFloat(getComputedStyle(target).getPropertyValue('--sq-radius'));
      svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
      svg.firstElementChild.setAttribute('d', shapePath(width, height, radius));
    }
  });
  for (const element of root.querySelectorAll('[data-squircle]')) {
    const svg = document.createElementNS(NS, 'svg');
    svg.classList.add('squircle-shape');
    svg.setAttribute('aria-hidden', 'true');
    svg.append(document.createElementNS(NS, 'path'));
    element.prepend(svg);
    element.classList.add('squircle-ready');
    observer.observe(element);
  }
}
