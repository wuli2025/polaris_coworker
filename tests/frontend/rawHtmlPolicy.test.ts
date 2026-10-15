import { describe, expect, it } from 'vitest';

// Eager raw imports include nested Vue files without compiling their templates.
const sources = import.meta.glob<string>([
  '../../src/features/interconnect/**/*.vue',
  '../../src/features/runtime/**/*.vue',
], { query: '?raw', import: 'default', eager: true });
const avatarView = '../../src/features/interconnect/InterconnectView.vue';
const staticAvatarImport = /import\s+allianceAvatar\s+from\s+["']\.\/alliance-avatar\.svg\?raw["']/;

function violations(path: string, source: string): string[] {
  let avatarCount = 0;
  return [...source.matchAll(/\bv-html\b(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?/g)]
    .filter(([attribute]) => {
      const allowed = path === avatarView && /^v-html\s*=\s*(["'])allianceAvatar\1$/.test(attribute)
        && staticAvatarImport.test(source);
      if (allowed) avatarCount += 1;
      return !allowed || avatarCount > 1;
    })
    .map(match => `${path}:${source.slice(0, match.index).split('\n').length}: ${match[0]}`);
}

describe('interconnect/runtime raw HTML policy', () => {
  it('scans both feature trees and permits only the static alliance avatar', () => {
    for (const feature of ['interconnect', 'runtime']) {
      expect(Object.keys(sources).some(path => path.includes(`/features/${feature}/`))).toBe(true);
    }
    expect(sources[avatarView]).toMatch(staticAvatarImport);
    expect(Object.entries(sources).flatMap(([path, source]) => violations(path, source))).toEqual([]);
  });

  it.each([
    '<div v-html="device.name" />',
    '<div v-html = \'device.name\' />',
    '<div\nv-html=\n"device.name" />',
    '<div v-html=device.name />',
    '<div v-html />',
    '<div v-html="allianceAvatar + device.name" />',
    '<div v-html:dynamic="allianceAvatar" />',
  ])('rejects dynamic/invalid raw HTML even in the avatar view: %s', template => {
    const source = `import allianceAvatar from "./alliance-avatar.svg?raw";\n${template}`;
    expect(violations(avatarView, source)).toHaveLength(1);
    expect(violations('../../src/features/runtime/Nested.vue', source)).toHaveLength(1);
  });

  it('limits the exception to one use in its exact file with the static SVG import', () => {
    const template = '<span v-html="allianceAvatar"></span>';
    const source = `import allianceAvatar from "./alliance-avatar.svg?raw";\n${template}`;
    expect(violations(avatarView, source)).toEqual([]);
    expect(violations(avatarView, template)).toHaveLength(1);
    expect(violations(avatarView, source + template)).toHaveLength(1);
    expect(violations('../../src/features/interconnect/nested/InterconnectView.vue', source)).toHaveLength(1);
  });
});
