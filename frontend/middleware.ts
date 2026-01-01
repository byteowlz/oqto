import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"

// Routes that don't require authentication
const publicRoutes = ["/login", "/register"]

// Routes that are API or static assets (should be skipped)
const skipRoutes = ["/api", "/_next", "/favicon.ico", "/ghostty-vt.wasm"]

export async function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl

  // Skip middleware for API routes, static assets, and public routes
  if (skipRoutes.some((route) => pathname.startsWith(route))) {
    return NextResponse.next()
  }

  if (publicRoutes.some((route) => pathname.startsWith(route))) {
    return NextResponse.next()
  }

  // Check if user has an auth token cookie
  // The backend uses JWT stored in auth_token cookie
  const authCookie = request.cookies.get("auth_token")

  if (!authCookie) {
    // No session cookie - redirect to login
    const loginUrl = new URL("/login", request.url)
    loginUrl.searchParams.set("redirect", pathname)
    return NextResponse.redirect(loginUrl)
  }

  // Session cookie exists - verify it's valid by calling the API
  // We do this server-side to avoid showing the dashboard briefly before redirect
  try {
    const meUrl = new URL("/api/me", request.url)
    const res = await fetch(meUrl.toString(), {
      headers: {
        cookie: request.headers.get("cookie") || "",
      },
    })

    if (res.status === 401) {
      // Session is invalid - redirect to login
      const loginUrl = new URL("/login", request.url)
      loginUrl.searchParams.set("redirect", pathname)
      return NextResponse.redirect(loginUrl)
    }
  } catch {
    // API not reachable - let the page handle this gracefully
    // Don't redirect, as the control plane might just be starting up
    return NextResponse.next()
  }

  return NextResponse.next()
}

export const config = {
  matcher: [
    /*
     * Match all request paths except:
     * - _next/static (static files)
     * - _next/image (image optimization files)
     * - favicon.ico (favicon file)
     * - public files (public folder)
     */
    "/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)",
  ],
}
