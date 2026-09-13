import { readFile } from 'node:fs/promises';
import { defineApplication } from '@isoastra/fleet-delivery';
import { planEnvironment } from '@isoastra/fleet-delivery/environment';
import { describe, expect, it } from 'vitest';

describe('delivery environment adoption', () => {
  it('keeps every workspace isolated and requires staging for this sensitive application', async () => {
    const definition = defineApplication(
      JSON.parse(await readFile('delivery.application.json', 'utf8')),
    );
    expect(definition.sensitivity).toBe('sensitive');
    expect(Object.values(definition.profiles).some(({ mode }) => mode === 'staging')).toBe(true);
    const first = planEnvironment(definition, {
      profile: 'local',
      owner: 'agent',
      workspace: '/work/one',
    });
    const second = planEnvironment(definition, {
      profile: 'local',
      owner: 'agent',
      workspace: '/work/two',
    });
    expect(first.namespace).not.toBe(second.namespace);
    expect(first.resources.map(({ port }) => port)).not.toEqual(
      second.resources.map(({ port }) => port),
    );
  });
});
