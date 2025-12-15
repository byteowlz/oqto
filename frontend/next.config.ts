import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  env: {
    // Base URL for the Caddy proxy (handles all container routing)
    NEXT_PUBLIC_CADDY_BASE_URL: process.env.NEXT_PUBLIC_CADDY_BASE_URL || "http://localhost",
    // Legacy direct URLs (for local dev without Caddy)
    NEXT_PUBLIC_OPENCODE_BASE_URL: process.env.NEXT_PUBLIC_OPENCODE_BASE_URL || "http://localhost:8080",
    NEXT_PUBLIC_FILESERVER_BASE_URL: process.env.NEXT_PUBLIC_FILESERVER_BASE_URL || "http://localhost:41821",
    NEXT_PUBLIC_TERMINAL_WS_URL: process.env.NEXT_PUBLIC_TERMINAL_WS_URL || "ws://localhost:41822",
  },
  async rewrites() {
    const caddyUrl = process.env.NEXT_PUBLIC_CADDY_BASE_URL || "http://localhost";
    // Fallback to direct URLs for local dev without Caddy
    const opencodeUrl = process.env.NEXT_PUBLIC_OPENCODE_BASE_URL || "http://localhost:41820";
    const fileserverUrl = process.env.NEXT_PUBLIC_FILESERVER_BASE_URL || "http://localhost:41821";
    
    return [
      // Container-specific routes via Caddy (production)
      {
        source: "/c/:containerId/api/:path*",
        destination: `${caddyUrl}/c/:containerId/api/:path*`,
      },
      {
        source: "/c/:containerId/files/:path*",
        destination: `${caddyUrl}/c/:containerId/files/:path*`,
      },
      {
        source: "/c/:containerId/term/:path*",
        destination: `${caddyUrl}/c/:containerId/term/:path*`,
      },
      // Legacy direct routes (local dev)
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
