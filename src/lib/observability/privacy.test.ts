import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import {
  PRIVATE_HYPERDX_CONFIG,
  PRIVATE_UMAMI_DESCRIPTOR,
  privateOperationalEvent,
} from './privacy';

describe('private observability policy', () => {
  it('disables content-bearing browser capture', () => {
    expect(PRIVATE_HYPERDX_CONFIG).toMatchObject({
      disableReplay: true,
      consoleCapture: false,
      advancedNetworkCapture: false,
      maskAllText: true,
    });
    expect(PRIVATE_UMAMI_DESCRIPTOR.src.endsWith('/script.js')).toBe(true);
    expect(PRIVATE_UMAMI_DESCRIPTOR.attributes).toMatchObject({
      'data-auto-track': 'false',
      'data-auto-pageview': 'false',
    });
  });

  it('rejects free text and unreviewed attributes before a vendor buffer', () => {
    expect(() =>
      privateOperationalEvent({
        name: 'event.opened',
        routeTemplate: '/e/:slug',
        attributes: { title: 'PRIVATE-EVENT-CANARY' },
      }),
    ).toThrow(/not allowlisted/);
    expect(
      privateOperationalEvent({
        name: 'event.opened',
        routeTemplate: '/e/:slug',
        attributes: { result: 'success', surface: 'guest' },
      }),
    ).toBeTruthy();
  });

  it('does not inject Umami on event or signed-in surfaces', () => {
    const caddy = readFileSync('deploy/ronitnath.com.caddy', 'utf8');
    const matcher = caddy
      .split(/\r?\n/u)
      .find((line) => line.trimStart().startsWith('@public_marketing path'));
    expect(matcher?.trim()).toBe('@public_marketing path / /about /about/*');
  });
});
