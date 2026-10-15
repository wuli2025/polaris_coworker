import { createRequire } from 'node:module';
import { cpSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve } from 'node:path';

// Use agentd's existing compiler parser, including export-from and literal dynamic imports.
const require = createRequire(new URL('../../agentd/package.json', import.meta.url));
const ts = require('typescript');
export function contractFiles(root, entry) {
  const found = new Set();
  function visit(path) {
    path = resolve(root, path);
    const name = relative(root, path);
    if (name.startsWith('..') || isAbsolute(name)) throw new Error(`Contract import outside root: ${name}`);
    if (found.has(name)) return;
    found.add(name);
    const source = readFileSync(path, 'utf8');
    for (const dependency of ts.preProcessFile(source, true, true).importedFiles) {
      if (dependency.fileName.startsWith('.')) visit(resolve(dirname(path), dependency.fileName));
    }
  }
  visit(entry);
  return [...found].sort();
}

export function copyContract(root, entry, destination) {
  for (const name of contractFiles(root, entry)) {
    const target = resolve(destination, name);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(resolve(root, name), target);
  }
}
