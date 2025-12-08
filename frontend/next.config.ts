import type { NextConfig } from "next";
import createNextIntlPlugin from "next-intl/plugin";

const withNextIntl = createNextIntlPlugin("./lib/i18n.ts");

const nextConfig: NextConfig = {
  env: {
    // Base URL for the Caddy proxy (handles all container routing)
    NEXT_PUBLIC_CADDY_BASE_URL: process.env.NEXT_PUBLIC_CADDY_BASE_URL || "",
    // Control plane backend (Axum) for session management and proxying
    NEXT_PUBLIC_CONTROL_PLANE_URL: process.env.NEXT_PUBLIC_CONTROL_PLANE_URL || "http://localhost:8080",
    // Legacy direct URLs (for local dev without control plane)
    NEXT_PUBLIC_OPENCODE_BASE_URL: process.env.NEXT_PUBLIC_OPENCODE_BASE_URL || "",
    NEXT_PUBLIC_FILE_SERVER_URL: process.env.NEXT_PUBLIC_FILE_SERVER_URL || "",
    NEXT_PUBLIC_TERMINAL_WS_URL: process.env.NEXT_PUBLIC_TERMINAL_WS_URL || "",
  },
  async rewrites() {
    const caddyUrl = process.env.NEXT_PUBLIC_CADDY_BASE_URL || "http://localhost";
    const controlPlaneUrl = process.env.NEXT_PUBLIC_CONTROL_PLANE_URL || "http://localhost:8080";
    // Fallback to direct URLs for local dev without Caddy
    const opencodeUrl = process.env.NEXT_PUBLIC_OPENCODE_BASE_URL || "http://localhost:41820";
    const fileserverUrl = process.env.NEXT_PUBLIC_FILE_SERVER_URL || "http://localhost:41821";
    
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
      // Control plane (dev/prod behind same origin)
      {
        source: "/api/:path*",
        destination: `${controlPlaneUrl}/:path*`,
      },
    ];
  },
};

export default withNextIntl(nextConfig);
