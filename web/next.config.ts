import type { NextConfig } from "next";

// In Docker, use the service name; locally use localhost
const API_URL = process.env.INTERNAL_API_URL || 'http://localhost:8080';

const nextConfig: NextConfig = {
  // Enable standalone output for Docker deployment
  output: 'standalone',
  // For development: proxy API calls to backend
  // Note: /schemas/* rewrite removed - it conflicts with page routes
  // The API client makes direct calls to the backend
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: `${API_URL}/api/:path*`,
      },
    ];
  },
};

export default nextConfig;
