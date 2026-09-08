import type { NextConfig } from 'next';

const config: NextConfig = {
  output: 'standalone',
  reactStrictMode: true,
  poweredByHeader: false,
  serverExternalPackages: ['openid-client'],
  env: { APP_VERSION: process.env.APP_VERSION ?? 'dev' },
  /* The streamed sky is content-addressed by build: `public/stars/lod/*` is
   * rebuilt only by `tools/starcat/build_star_lod.py` against a pinned Gaia
   * release, and a rebuild would change the file names in the manifest. So the
   * tiles may be cached for a year and never revalidated — which is what makes
   * a second visit cost nothing and a `force-cache` fetch land locally.
   *
   * `bright.bin` deliberately does not get this: it is rebuilt per release
   * under the same name, and its default validators are what let a new
   * catalogue reach a returning visitor. */
  async headers() {
    return [
      {
        source: '/stars/lod/:path*',
        headers: [{ key: 'Cache-Control', value: 'public, max-age=31536000, immutable' }],
      },
    ];
  },
};

export default config;
