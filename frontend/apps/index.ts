import { appRegistry } from "@/lib/app-registry";
import { ProjectsApp } from "@/apps/projects";
import { AgentsApp } from "@/apps/agents";
import { SessionsApp } from "@/apps/sessions";
import { AdminApp } from "@/apps/admin";

appRegistry
  .register({
    id: "sessions",
    label: { de: "Chats", en: "Chats" },
    description: "Monitor live agent sessions",
    component: SessionsApp,
    routes: ["/sessions"],
    permissions: ["user"],
    priority: 1,
  })
  .register({
    id: "agents",
    label: { de: "Agenten", en: "Agents" },
    description: "AI agents for different tasks",
    component: AgentsApp,
    routes: ["/agents"],
    permissions: ["user"],
    priority: 5,
  })
  .register({
    id: "projects",
    label: { de: "Projekte", en: "Projects" },
    description: "Collaborate with others in a joined space",
    component: ProjectsApp,
    routes: ["/projects"],
    permissions: ["user"],
    priority: 20,
  })
  .register({
    id: "admin",
    label: { de: "Admin Space", en: "Admin Space" },
    description: "Platform telemetry and controls",
    component: AdminApp,
    routes: ["/admin"],
    permissions: ["admin"],
    priority: 30,
  });

export const registeredApps = appRegistry.getAllApps();
