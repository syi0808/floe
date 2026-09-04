import assert from 'node:assert/strict';
import { readFile, readdir, access } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const catalog = JSON.parse(await readFile(resolve(root, 'src/components/catalog.json'), 'utf8'));
const ids = new Set();
const registered = new Set();

assert.equal(catalog.schemaVersion, 1);
for (const entry of catalog.entries) {
  assert(!ids.has(entry.id), `Duplicate component ID: ${entry.id}`);
  ids.add(entry.id);
  const identity = `${entry.source}:${entry.export}`;
  assert(!registered.has(identity), `Duplicate export: ${identity}`);
  registered.add(identity);
  const source = await readFile(resolve(root, entry.source), 'utf8');
  const signature = source.match(
    new RegExp(`export function ${entry.export}\\(\\s*(?:\\{([\\s\\S]*?)\\})?\\s*\\)`),
  );
  assert(signature, `Missing component export: ${identity}`);
  const props = (signature[1] || '')
    .split(',')
    .map((prop) => prop.trim().split(/\s*[=:]\s*/)[0])
    .filter(Boolean);
  assert.deepEqual(props, entry.props, `Props drift: ${identity}`);
  assert(entry.responsibility && entry.stateOwner && entry.kind, `Missing contract: ${identity}`);
  assert(Array.isArray(entry.states) && entry.styles.length, `Missing metadata: ${identity}`);
  for (const path of [...entry.styles, entry.spec]) await access(resolve(root, path));
}

async function componentFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) return componentFiles(path);
      return path.endsWith('.jsx') ? [path] : [];
    }),
  );
  return nested.flat();
}

const sources = [
  ...(await componentFiles(resolve(root, 'src/components'))),
  ...['App.jsx', 'CalendarScreen.jsx', 'ProgressScreen.jsx', 'primitives.jsx'].map((name) =>
    resolve(root, 'src', name),
  ),
];
for (const path of sources) {
  const source = await readFile(path, 'utf8');
  for (const match of source.matchAll(/export function (\w+)\(/g)) {
    const identity = `${path.slice(root.length + 1)}:${match[1]}`;
    assert(registered.has(identity), `Unregistered component: ${identity}`);
  }
}
console.log(
  `Component catalog verified: ${catalog.entries.length} exports, props and source/style/spec paths.`,
);
