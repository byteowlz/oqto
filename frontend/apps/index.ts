import { appRegistry } from "@/lib/app-registry"
import { WorkspacesApp } from "@/apps/workspaces"
import { SessionsApp } from "@/apps/sessions"
import { AdminApp } from "@/apps/admin"

appRegistry
  .register({
    id: "workspaces",
    label: "Workspaces",
    description: "Manage and launch project workspaces",
    component: WorkspacesApp,
    routes: ["/workspaces"],
    permissions: ["user"],
    priority: 10,
  })
  .register({
    id: "sessions",
    label: "Active Sessions",
    description: "Monitor live agent sessions",
    component: SessionsApp,
    routes: ["/sessions"],
    permissions: ["user"],
    priority: 20,
  })
  .register({
    id: "admin",
    label: "Admin Panel",
    description: "Platform telemetry and controls",
    component: AdminApp,
    routes: ["/admin"],
    permissions: ["admin"],
    priority: 30,
  })

export const registeredApps = appRegistry.getAllApps()
