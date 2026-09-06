import type { NextConfig } from 'next';

const config: NextConfig = {
  output: 'standalone',
  reactStrictMode: true,
  poweredByHeader: false,
  env: { APP_VERSION: process.env.APP_VERSION ?? 'dev' },
};

export default config;
