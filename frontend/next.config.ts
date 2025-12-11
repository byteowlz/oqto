import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  env: {
    NEXT_PUBLIC_OPENCODE_BASE_URL: process.env.NEXT_PUBLIC_OPENCODE_BASE_URL || "http://localhost:8080",
    NEXT_PUBLIC_FILESERVER_BASE_URL: process.env.NEXT_PUBLIC_FILESERVER_BASE_URL || "http://localhost:8081",
  },
  async rewrites() {
    const opencodeUrl = process.env.NEXT_PUBLIC_OPENCODE_BASE_URL || "http://localhost:8080";
    const fileserverUrl = process.env.NEXT_PUBLIC_FILESERVER_BASE_URL || "http://localhost:8081";
    return [
      {
        source: "/api/opencode/:path*",
        destination: `${opencodeUrl}/:path*`,
      },
      {
        source: "/api/files/:path*",
        destination: `${fileserverUrl}/:path*`,
      },
    ];
  },
};

export default nextConfig;
