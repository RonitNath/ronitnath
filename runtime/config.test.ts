import { afterEach, describe, expect, it } from 'vitest';
import { runtimeConfig } from './config';

const original = { ...process.env };
afterEach(() => { process.env = { ...original }; });

describe('runtimeConfig', () => {
  it('rejects split databases and invalid ports', () => {
    Object.assign(process.env, { NODE_ENV: 'production', DATABASE_URL: 'postgres://one',
      DBOS_SYSTEM_DATABASE_URL: 'postgres://two', APP_VERSION: 'v1', REPLICA_ID: 'a', PUBLIC_ORIGIN: 'https://example.test' });
    expect(() => runtimeConfig()).toThrow('must equal DATABASE_URL');
    process.env.DBOS_SYSTEM_DATABASE_URL = process.env.DATABASE_URL;
    process.env.PORT = '0';
    expect(() => runtimeConfig()).toThrow('PORT must be');
  });

  it('requires stable production identity and an origin', () => {
    Object.assign(process.env, { NODE_ENV: 'production', DATABASE_URL: 'postgres://one' });
    delete process.env.APP_VERSION;
    delete process.env.REPLICA_ID;
    delete process.env.PUBLIC_ORIGIN;
    expect(() => runtimeConfig()).toThrow('APP_VERSION');
  });
});
