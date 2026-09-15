import {
  assertProfileCompatible,
  hyperDxConfig,
  umamiScriptDescriptor,
  validateOperationalEvent,
} from '@isoastra/privacy-observability';

export const PRIVATE_LEVEL = 'managed' as const;
export const PRIVATE_PROFILE = 'restricted' as const;
export const PRIVATE_OPERATIONAL_ATTRIBUTES = ['result', 'surface'] as const;

assertProfileCompatible(PRIVATE_LEVEL, PRIVATE_PROFILE);

/** Configuration for a future HyperDX browser integration. Keeping this
 * beside the application policy makes replay/console/network defaults
 * reviewable before any SDK is added. */
export const PRIVATE_HYPERDX_CONFIG = hyperDxConfig(PRIVATE_PROFILE);

/** Umami is edge-loaded on public marketing pages only. This descriptor is
 * the qualified restricted configuration to use if tracking later moves into
 * the application. autoTrack and autoPageview are deliberately false. */
export const PRIVATE_UMAMI_DESCRIPTOR = umamiScriptDescriptor({
  origin: 'https://analytics.isoastra.com',
  websiteId: 'ba50852f-45e9-43e9-983c-ceaa773bc7dd',
  profile: PRIVATE_PROFILE,
  vendorVersion: '3.3.1',
});

export function privateOperationalEvent(
  event: Parameters<typeof validateOperationalEvent>[0],
) {
  return validateOperationalEvent(event, {
    allowedAttributes: PRIVATE_OPERATIONAL_ATTRIBUTES,
  });
}
