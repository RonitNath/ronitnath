import type { NextConfig } from 'next';

const IMMUTABLE = 'public, max-age=31536000, immutable';

const config: NextConfig = {
  output: 'standalone',
  reactStrictMode: true,
  poweredByHeader: false,
  serverExternalPackages: ['openid-client'],
  env: { APP_VERSION: process.env.APP_VERSION ?? 'dev' },
  /* Every sky asset is content-addressed by its builder — the star catalogue,
   * the names, the figures, the band, the city list, the globe's textures, the
   * 768 streamed tiles and the manifest that names them all carry the sha256
   * of their own bytes in their filename (`tools/starcat/name_assets.py`,
   * `build_star_lod.py`). A rebuild writes a different URL, so these may be
   * cached for a year and never revalidated: a deploy cannot invalidate a byte
   * and no cache ever has to be purged.
   *
   * Before this, only `/stars/lod/*` was immutable and every other asset came
   * back `REVALIDATED` from Cloudflare: nine conditional GETs to the origin on
   * every page load, answered with a 304 each. Cheap in bytes, a round trip
   * each in latency, and unnecessary for a file whose name is its hash.
   * `docs/sky-cdn.md` measured it. */
  async headers() {
    return [
      {
        source: '/:dir(stars|sky|cities|textures)/:path*',
        headers: [{ key: 'Cache-Control', value: IMMUTABLE }],
      },
    ];
  },
};

export default config;
