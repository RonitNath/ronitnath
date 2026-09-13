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
    expect(definition.fixtures.find(({ name }) => name === 'synthetic')?.command?.args).toEqual([
      'db:migrate',
    ]);
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

  it('keeps production and staging credentials behind branch-scoped environments', async () => {
    const workflow = await readFile('.github/workflows/deploy.yml', 'utf8');
    expect(workflow).toContain("branches: ['deploy', 'staging', 'preview/**']");
    expect(workflow).toContain('@isoastra/fleet-delivery@0.3.13');
    expect(workflow).toContain("github.ref_name == 'deploy' && 'production' || 'staging'");
    expect(workflow).toContain('docker/login-action@dbcb813823bdd20940b903addbd779551569679f');
    expect(workflow).toContain("startsWith(github.ref, 'refs/heads/preview/') && 'preview' || 'staging'");
  });
});
