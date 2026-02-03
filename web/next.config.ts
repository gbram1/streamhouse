import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Enable standalone output for Docker deployment
  output: 'standalone',
  // For development: proxy API calls to backend
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: 'http://localhost:8080/api/:path*',
      },
      {
        source: '/schemas/:path*',
        destination: 'http://localhost:8080/schemas/:path*',
      },
    ];
  },
};

export default nextConfig;
