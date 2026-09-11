import type { NextConfig } from 'next';

const IMMUTABLE = 'public, max-age=31536000, immutable';

const config: NextConfig = {
  output: 'standalone',
  reactStrictMode: true,
  poweredByHeader: false,
  env: { APP_VERSION: process.env.APP_VERSION ?? 'dev' },
  /* The two halves of the old grammar that need no session.
   *
   * `/org/<handle>` and `/platform` named an object and an audience that both
   * still exist under their new names, so the mapping is a pure function of
   * the path and belongs here rather than in a route: it costs no render, it
   * is answered before the app is reached, and a 308 tells every cache that
   * the old path is gone for good.
   *
   * `/app/...` is the exception and is deliberately absent. Its destination
   * is `/u/<the reader's own public id>/...`, which nothing static can know —
   * it is resolved from the session in `src/app/app/[[...rest]]/page.tsx`. */
  async redirects() {
    return [
      { source: '/org/:handle', destination: '/o/:handle', permanent: true },
      { source: '/org/:handle/:path*', destination: '/o/:handle/:path*', permanent: true },
      { source: '/platform', destination: '/o/isoastra', permanent: true },
      { source: '/platform/:path*', destination: '/o/isoastra/:path*', permanent: true },
    ];
  },

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
