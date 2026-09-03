import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const endpoint = process.env.CHROME_CDP ?? 'http://127.0.0.1:9223';
const url = process.env.PROTOTYPE_URL ?? 'http://127.0.0.1:5173/';
const output = resolve(process.argv[2] ?? 'build/visual-parity');
const page = await (await fetch(`${endpoint}/json/new?about:blank`, { method: 'PUT' })).json();
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});
let sequence = 0;
const pending = new Map();
socket.onmessage = ({ data }) => {
  const message = JSON.parse(data);
  const request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id);
  if (message.error) request.reject(new Error(JSON.stringify(message.error)));
  else request.resolve(message.result);
};
const call = (method, params = {}) => new Promise((resolve, reject) => {
  const id = ++sequence;
  pending.set(id, { resolve, reject });
  socket.send(JSON.stringify({ id, method, params }));
});
const evaluate = (expression) => call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
const pause = () => new Promise((resolve) => setTimeout(resolve, 600));
await mkdir(output, { recursive: true });
const measurements = [];
try {
  await call('Page.enable');
  for (const [width, height] of [[1440, 1100], [390, 844]]) {
    await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: width <= 780 });
    for (const screen of ['today', 'suggestion', 'notes', 'task']) {
      await call('Page.navigate', { url });
      await pause();
      await evaluate('document.fonts.ready');
      if (screen === 'suggestion') await evaluate('document.querySelector(".floe-button").click()');
      if (screen === 'notes') await evaluate('document.querySelector(".nav-link[title=Notes]").click()');
      if (screen === 'task') await evaluate('document.querySelector(".nav-link[title=Tasks]").click()');
      await pause();
      const screenshot = await call('Page.captureScreenshot');
      await writeFile(`${output}/prototype-${screen}-${width}.png`, Buffer.from(screenshot.data, 'base64'));
      const metrics = await evaluate(`JSON.stringify([...document.querySelectorAll('.local-toolbar,.timeline-border,.rail-card-border,.note-preview-border,.detail-panel-border')].map(element => ({name: element.className, rect: element.getBoundingClientRect().toJSON()})))`);
      measurements.push({ screen, width, height, elements: JSON.parse(metrics.result.value) });
    }
  }
  await writeFile(`${output}/prototype-measurements.json`, JSON.stringify(measurements, null, 2));
} finally {
  socket.close();
  await fetch(`${endpoint}/json/close/${page.id}`);
}
console.log(`Prototype captures: ${output}`);
