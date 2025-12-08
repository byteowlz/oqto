# App Module Guide

This frontend treats each sidebar entry as a standalone “app” module. Every app lives in `apps/<app-id>/` and registers itself through `apps/index.ts`. This document covers the minimal steps to add a new entry to the registry and hook it into the shared layout.

## 1. Create the Module

```
apps/
  your-app/
    index.tsx   // export function YourApp() { … }
```

Guidelines:

- Components must be client-side (`"use client"`) if they rely on hooks or browser APIs.
- Keep internal state and data fetching self-contained so other apps are unaffected.
- Reuse shared UI primitives under `components/ui/` and helper libraries under `lib/`.

## 2. Register the App

Open `apps/index.ts` and add an entry:

```ts
import { YourApp } from "@/apps/your-app"
import { appRegistry } from "@/lib/app-registry"

appRegistry.register({
  id: "your-app",
  label: "Your App",
  description: "Short summary",
  component: YourApp,
  routes: ["/your-app"],    // optional metadata
  permissions: ["user"],    // optional, used for future auth filtering
  priority: 40,              // lower number = higher in the list
})
```

The `AppProvider` (used in `app/page.tsx`) reads the registry exactly once at startup, so importing `@/apps` is enough to populate the sidebar.

## 3. Verify the Layout Metadata

Each `AppDefinition` field has a purpose:

| Field | Description |
| ----- | ----------- |
| `id` | Stable key (used for routing, persisted selections). |
| `label` | Display name rendered in the sidebar. |
| `description` | Optional helper text (shows under the label when the sidebar is expanded). |
| `component` | React component to mount inside the main content area. |
| `routes` | Optional array for future routing integration. |
| `permissions` | Optional list for role-based filtering. |
| `defaultState` | Reserved placeholder if your app needs initial settings. |
| `priority` | Sorting weight (lower first). |

## 4. Add Supporting Files (Optional)

- Share hooks/utilities via `lib/` if multiple apps will reuse them.
- Place dedicated components under `apps/your-app/components/` if they are app-specific.
- Update documentation (`README.md` or `README_apps.md`) if the new app introduces config or env vars.

## 5. Test

1. `bun dev` – ensure the new entry appears in the sidebar and renders correctly.
2. `bun lint` – lint must stay green.
3. If the app calls remote services, gate anything network-dependent behind configuration checks (see `lib/config.ts`).

That’s it—once the module is registered, the shared layout automatically renders it and persists the active selection for the user session.
