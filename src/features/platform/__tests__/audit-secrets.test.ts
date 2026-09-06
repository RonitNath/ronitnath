/* No audit payload carries a secret.
 *
 * The audit feed is rendered whole on `/platform/audit`, so a payload that
 * names a token, a password hash or a session token would put it on a page —
 * and unlike a query, a payload is written by hand at every command site,
 * which is exactly the kind of thing that is right forty times and wrong
 * once. This reads the command sources themselves: every `payload: { … }`
 * literal in the tree, and every key inside one.
 *
 * It is a source scan rather than a run of every command because the property
 * is about what a programmer may write, not about what one seeded row
 * happened to contain. */

import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const ROOTS = ['src', 'scripts'];
const FORBIDDEN = /token|password|secret|hash|phc|credential|cookie/i;
/* Names that contain a forbidden word and are demonstrably not one: an id of
 * the session a command minted is a public handle, not the key to it. */
const ALLOWED = new Set<string>([]);

async function sources(dir: string): Promise<string[]> {
  const out: string[] = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...(await sources(path)));
    else if (/\.tsx?$/.test(entry.name)) out.push(path);
  }
  return out;
}

/** Every `payload: { … }` literal in one file, balanced. */
function payloads(source: string): string[] {
  const out: string[] = [];
  const marker = 'payload: {';
  let at = source.indexOf(marker);
  while (at >= 0) {
    let depth = 0;
    let index = at + marker.length - 1;
    for (; index < source.length; index += 1) {
      if (source[index] === '{') depth += 1;
      else if (source[index] === '}') {
        depth -= 1;
        if (depth === 0) break;
      }
    }
    out.push(source.slice(at + marker.length, index));
    at = source.indexOf(marker, index);
  }
  return out;
}

/** The `key: value` pairs of one object literal body, at its top level. */
function pairs(body: string): { key: string; value: string }[] {
  const out: { key: string; value: string }[] = [];
  let depth = 0;
  let start = 0;
  const parts: string[] = [];
  for (let index = 0; index < body.length; index += 1) {
    const char = body[index];
    if (char === '{' || char === '[' || char === '(') depth += 1;
    else if (char === '}' || char === ']' || char === ')') depth -= 1;
    else if (char === ',' && depth === 0) {
      parts.push(body.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(body.slice(start));
  for (const part of parts) {
    const found = /^\s*(?:\/\*[\s\S]*?\*\/\s*)*([A-Za-z_][A-Za-z0-9_]*)\s*:([\s\S]*)$/.exec(part);
    if (found?.[1]) out.push({ key: found[1], value: (found[2] ?? '').trim() });
  }
  return out;
}

function keys(body: string): string[] {
  return pairs(body).map((pair) => pair.key);
}

/* A value that *is* a secret, rather than a value computed from whether one
 * was presented: `via: token ? 'link' : 'session'` records the door, not the
 * key to it. */
const SPELLED = /^[A-Za-z0-9_.]*\b(?:token|tokenHash|secret|phc|passwordHash)$/;

describe('audit payloads', () => {
  it('never name a token, a password, a hash or any other secret', async () => {
    const files = (await Promise.all(ROOTS.map(sources))).flat();
    const offenders: string[] = [];

    for (const file of files) {
      if (file.includes('__tests__')) continue;
      const source = await readFile(file, 'utf8');
      for (const body of payloads(source)) {
        for (const { key, value } of pairs(body)) {
          if (FORBIDDEN.test(key) && !ALLOWED.has(key)) offenders.push(`${file}: ${key}`);
          /* And nothing spelled as a value either: a payload that stores
           * `factor.secret` names no forbidden key at all. */
          if (SPELLED.test(value)) offenders.push(`${file}: value ${value}`);
        }
      }
    }

    expect(offenders).toEqual([]);
  });

  it('finds the literals it claims to read', async () => {
    const source = await readFile(join('src', 'features', 'platform', 'actions.ts'), 'utf8');
    const found = payloads(source);
    expect(found.length).toBeGreaterThan(5);
    expect(keys(found[0]!)).toContain('source');
  });
});
