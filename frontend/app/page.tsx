"use client";

import { useEffect, useState, useCallback, useMemo, useDeferredValue } from "react";
import { useTheme } from "next-themes";
import Image from "next/image";
import {
  SunMedium,
  MoonStar,
  Globe2,
  PanelLeftClose,
  PanelRightClose,
  FolderKanban,
  MessageSquare,
  Bot,
  Shield,
  Menu,
  X,
  Clock,
  Pin,
  Pencil,
  Trash2,
  Plus,
  ChevronRight,
  ChevronDown,
  Copy,
  Search,
  Loader2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { AppProvider, useApp } from "@/components/app-context";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { CommandPalette, useCommandPalette } from "@/components/command-palette";
import { AgentPicker } from "@/components/agent-picker";
import { fetchAgents, type OpenCodeAgent } from "@/lib/opencode-client";
import { generateReadableId, formatSessionDate } from "@/lib/session-utils";
import { listWorkspaceDirectories, type Persona, type ChatSession } from "@/lib/control-plane-client";
import "@/apps";

function AppShell() {
  const {
    apps,
    activeAppId,
    setActiveAppId,
    activeApp,
    locale,
    setLocale,
    resolveText,
    chatHistory,
    opencodeSessions,
    selectedChatSessionId,
    setSelectedChatSessionId,
    selectedChatSession,
    selectedChatFromHistory,
    selectedWorkspaceSession,
    opencodeBaseUrl,
    ensureOpencodeRunning,
    createNewChat,
    createNewChatWithPersona,
    deleteChatSession,
    renameChatSession,
    busySessions,
  } = useApp();
  const { theme, setTheme, resolvedTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  
  // Avoid hydration mismatch - only render theme-dependent content after mount
  useEffect(() => {
    setMounted(true);
  }, []);
  
  // Use a stable theme value that defaults to "light" during SSR to prevent hydration mismatch
  const currentTheme = mounted ? resolvedTheme : "light";
  const isDark = currentTheme === "dark";
  
  const ActiveComponent = activeApp?.component ?? null;

  // Loading bar
  const [barVisible, setBarVisible] = useState(true);
  const [barWidth, setBarWidth] = useState(0);
  const [barFade, setBarFade] = useState(false);

  // Dialog states
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [renameDialogOpen, setRenameDialogOpen] = useState(false);
  const [agentPickerOpen, setAgentPickerOpen] = useState(false);
  const [targetSessionId, setTargetSessionId] = useState<string>("");
  const [renameValue, setRenameValue] = useState("");

  // Command palette
  const { open: commandPaletteOpen, setOpen: setCommandPaletteOpen } = useCommandPalette();

  // Expanded state for parent sessions in sidebar
  const [expandedSessions, setExpandedSessions] = useState<Set<string>>(new Set());

  // Pinned sessions (persisted to localStorage)
  const [pinnedSessions, setPinnedSessions] = useState<Set<string>>(() => {
    if (typeof window === "undefined") return new Set();
    try {
      const stored = localStorage.getItem("octo:pinnedSessions");
      return stored ? new Set(JSON.parse(stored)) : new Set();
    } catch {
      return new Set();
    }
  });

  // Persist pinned sessions to localStorage
  useEffect(() => {
    if (typeof window === "undefined") return;
    localStorage.setItem("octo:pinnedSessions", JSON.stringify([...pinnedSessions]));
  }, [pinnedSessions]);

  const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(null);
  const [availableAgents, setAvailableAgents] = useState<OpenCodeAgent[]>([]);
  const [projectDefaultAgents, setProjectDefaultAgents] = useState<Record<string, string>>(() => {
    if (typeof window === "undefined") return {};
    try {
      const stored = localStorage.getItem("octo:projectDefaultAgents");
      return stored ? JSON.parse(stored) : {};
    } catch {
      return {};
    }
  });
  const [workspaceDirectories, setWorkspaceDirectories] = useState<{ name: string; path: string }[]>([]);
  const [directoryPickerOpen, setDirectoryPickerOpen] = useState(false);
  const [directoryPickerPath, setDirectoryPickerPath] = useState(".");
  const [directoryPickerEntries, setDirectoryPickerEntries] = useState<{ name: string; path: string }[]>([]);
  const [directoryPickerLoading, setDirectoryPickerLoading] = useState(false);
  const [pendingPersona, setPendingPersona] = useState<Persona | null>(null);

  const handleProjectDefaultAgentChange = useCallback((projectKey: string, agentId: string) => {
    setProjectDefaultAgents((prev) => {
      if (!agentId) {
        const next = { ...prev };
        delete next[projectKey];
        return next;
      }
      return { ...prev, [projectKey]: agentId };
    });
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;
    localStorage.setItem("octo:projectDefaultAgents", JSON.stringify(projectDefaultAgents));
  }, [projectDefaultAgents]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const handleFilter = (event: Event) => {
      const customEvent = event as CustomEvent<string>;
      if (typeof customEvent.detail === "string") {
        setSelectedProjectKey(customEvent.detail);
        setActiveAppId("sessions");
      }
    };
    const handleClear = () => {
      setSelectedProjectKey(null);
    };
    const handleDefaultAgent = (event: Event) => {
      const customEvent = event as CustomEvent<{ projectKey: string; agentId: string }>;
      if (!customEvent.detail) return;
      handleProjectDefaultAgentChange(customEvent.detail.projectKey, customEvent.detail.agentId);
    };

    window.addEventListener("octo:project-filter", handleFilter as EventListener);
    window.addEventListener("octo:project-filter-clear", handleClear as EventListener);
    window.addEventListener("octo:project-default-agent", handleDefaultAgent as EventListener);
    return () => {
      window.removeEventListener("octo:project-filter", handleFilter as EventListener);
      window.removeEventListener("octo:project-filter-clear", handleClear as EventListener);
      window.removeEventListener("octo:project-default-agent", handleDefaultAgent as EventListener);
    };
  }, [handleProjectDefaultAgentChange, setActiveAppId]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    listWorkspaceDirectories(".")
      .then((entries) => {
        const dirs = entries.map((entry) => ({ name: entry.name, path: entry.path }));
        setWorkspaceDirectories(dirs);
      })
      .catch((err) => {
        console.error("Failed to load workspace directories:", err);
        setWorkspaceDirectories([]);
      });
  }, []);

  useEffect(() => {
    if (!directoryPickerOpen || typeof window === "undefined") return;
    setDirectoryPickerLoading(true);
    listWorkspaceDirectories(directoryPickerPath)
      .then((entries) => {
        const dirs = entries.map((entry) => ({ name: entry.name, path: entry.path }));
        setDirectoryPickerEntries(dirs);
      })
      .catch((err) => {
        console.error("Failed to load directory picker entries:", err);
        setDirectoryPickerEntries([]);
      })
      .finally(() => setDirectoryPickerLoading(false));
  }, [directoryPickerOpen, directoryPickerPath]);

  // Session search
  const [sessionSearch, setSessionSearch] = useState("");
  const deferredSearch = useDeferredValue(sessionSearch);

  // Build hierarchical session structure from chatHistory (disk-based, no opencode needed)
  const sessionHierarchy = useMemo(() => {
    // Separate parent and child sessions
    const parentSessions = chatHistory.filter((s) => !s.parent_id);
    const childSessionsByParent = new Map<string, ChatSession[]>();
    
    for (const session of chatHistory) {
      if (session.parent_id) {
        const children = childSessionsByParent.get(session.parent_id) || [];
        children.push(session);
        childSessionsByParent.set(session.parent_id, children);
      }
    }
    
    // Sort children by updated time (newest first)
    for (const [parentId, children] of childSessionsByParent) {
      childSessionsByParent.set(
        parentId,
        children.sort((a, b) => b.updated_at - a.updated_at)
      );
    }
    
    return { parentSessions, childSessionsByParent };
  }, [chatHistory]);

  // Helper to get project key from ChatSession (disk-based) or OpenCodeSession (live)
  const projectKeyForSession = useCallback(
    (session: ChatSession | { directory?: string | null; projectID?: string | null }) => {
      // ChatSession type uses workspace_path/project_name
      if ('workspace_path' in session && session.workspace_path) {
        const normalized = session.workspace_path.replace(/\\/g, "/").replace(/\/+$/, "");
        const parts = normalized.split("/").filter(Boolean);
        return parts[parts.length - 1] ?? session.workspace_path;
      }
      // OpenCodeSession type uses directory/projectID
      const directory = ('directory' in session ? session.directory : null)?.trim();
      if (directory) {
        const normalized = directory.replace(/\\/g, "/").replace(/\/+$/, "");
        const parts = normalized.split("/").filter(Boolean);
        return parts[parts.length - 1] ?? directory;
      }
      const projectId = ('projectID' in session ? session.projectID : null)?.trim();
      if (projectId) return projectId;
      return "workspace";
    },
    [],
  );

  const projectLabelForSession = useCallback(
    (session: ChatSession | { directory?: string | null; projectID?: string | null }) => {
      // ChatSession type uses project_name directly
      if ('project_name' in session && session.project_name) {
        return session.project_name;
      }
      // OpenCodeSession type uses directory/projectID
      const directory = ('directory' in session ? session.directory : null)?.trim();
      if (directory) {
        const normalized = directory.replace(/\\/g, "/");
        const parts = normalized.split("/").filter(Boolean);
        return parts[parts.length - 1] ?? directory;
      }
      const projectId = ('projectID' in session ? session.projectID : null)?.trim();
      if (projectId) return projectId;
      return locale === "de" ? "Arbeitsbereich" : "Workspace";
    },
    [locale],
  );

  useEffect(() => {
    if (!opencodeBaseUrl) return;
    fetchAgents(opencodeBaseUrl)
      .then((agents) => {
        setAvailableAgents(agents);
      })
      .catch((err) => {
        console.error("Failed to fetch agents:", err);
        setAvailableAgents([]);
      });
  }, [opencodeBaseUrl]);

  // Filter and sort sessions (pinned first, then by recency)
  const filteredSessions = useMemo(() => {
    const searchLower = deferredSearch.toLowerCase().trim();
    let sessions = sessionHierarchy.parentSessions;

    if (selectedProjectKey) {
      sessions = sessions.filter((session) => projectKeyForSession(session) === selectedProjectKey);
    }
    
    if (searchLower) {
      sessions = sessions.filter((session) => {
        // Search in title
        if (session.title?.toLowerCase().includes(searchLower)) return true;
        // Search in readable ID (adjective-noun)
        const readableId = generateReadableId(session.id);
        if (readableId.toLowerCase().includes(searchLower)) return true;
        // Search in date (ChatSession uses updated_at)
        if (session.updated_at) {
          const dateStr = formatSessionDate(session.updated_at);
          if (dateStr.toLowerCase().includes(searchLower)) return true;
        }
        return false;
      });
    }
    
    // Sort: pinned first, then by updated time (ChatSession uses updated_at)
    return [...sessions].sort((a, b) => {
      const aPinned = pinnedSessions.has(a.id);
      const bPinned = pinnedSessions.has(b.id);
      if (aPinned && !bPinned) return -1;
      if (!aPinned && bPinned) return 1;
      return b.updated_at - a.updated_at;
    });
  }, [sessionHierarchy.parentSessions, deferredSearch, pinnedSessions, projectKeyForSession, selectedProjectKey]);

  const projectSummaries = useMemo(() => {
    const entries = new Map<
      string,
      { key: string; name: string; directory?: string; sessionCount: number; lastActive: number }
    >();

    for (const directory of workspaceDirectories) {
      entries.set(directory.path, {
        key: directory.path,
        name: directory.name,
        directory: directory.path,
        sessionCount: 0,
        lastActive: 0,
      });
    }

    for (const session of sessionHierarchy.parentSessions) {
      const key = projectKeyForSession(session);
      const name = projectLabelForSession(session);
      // ChatSession uses updated_at instead of time.updated
      const lastActive = session.updated_at ?? 0;
      const existing = entries.get(key);
      if (existing) {
        existing.sessionCount += 1;
        if (lastActive > existing.lastActive) existing.lastActive = lastActive;
        // Update directory to absolute path from session if available
        if (session.workspace_path && !existing.directory?.startsWith("/")) {
          existing.directory = session.workspace_path;
        }
      } else {
        entries.set(key, {
          key,
          name,
          // ChatSession uses workspace_path instead of directory
          directory: session.workspace_path ?? undefined,
          sessionCount: 1,
          lastActive,
        });
      }
    }

    if (!entries.has("workspace")) {
      entries.set("workspace", {
        key: "workspace",
        name: locale === "de" ? "Arbeitsbereich" : "Workspace",
        sessionCount: 0,
        lastActive: 0,
      });
    }

    return [...entries.values()].sort((a, b) => b.lastActive - a.lastActive);
  }, [projectKeyForSession, projectLabelForSession, sessionHierarchy.parentSessions, workspaceDirectories, locale]);

  const selectedProjectLabel = useMemo(() => {
    if (!selectedProjectKey) return null;
    return projectSummaries.find((project) => project.key === selectedProjectKey)?.name ?? selectedProjectKey;
  }, [projectSummaries, selectedProjectKey]);

  const directoryPickerParent = useMemo(() => {
    const normalized = directoryPickerPath.replace(/\\/g, "/");
    if (normalized === "." || normalized === "") return null;
    const parts = normalized.split("/").filter(Boolean);
    if (parts.length <= 1) return ".";
    return parts.slice(0, -1).join("/");
  }, [directoryPickerPath]);

  const recentProjects = useMemo(() => projectSummaries.slice(0, 6), [projectSummaries]);

  const toggleSessionExpanded = useCallback((sessionId: string) => {
    setExpandedSessions((prev) => {
      const next = new Set(prev);
      if (next.has(sessionId)) {
        next.delete(sessionId);
      } else {
        next.add(sessionId);
      }
      return next;
    });
  }, []);

  const handleProjectSelect = useCallback(
    (projectKey: string) => {
      setSelectedProjectKey(projectKey);
      setActiveAppId("sessions");
      setMobileMenuOpen(false);
    },
    [setActiveAppId],
  );

  const handleProjectClear = useCallback(() => {
    setSelectedProjectKey(null);
  }, []);

  const resolveWorkspacePath = useCallback(
    (path: string) => {
      const basePath = selectedWorkspaceSession?.workspace_path;
      if (!basePath) return path;
      if (path.startsWith("/")) return path;
      if (path === "." || path.trim() === "") return basePath;
      const joined = `${basePath}/${path}`;
      const normalized = joined.split("/").filter(Boolean).join("/");
      return basePath.startsWith("/") ? `/${normalized}` : normalized;
    },
    [selectedWorkspaceSession?.workspace_path],
  );

  const handleDirectoryConfirm = useCallback(
    async (path: string) => {
      if (!pendingPersona) return;
      // The path from directory picker is already relative to workspace root
      // Don't resolve it relative to current session - pass it directly
      // The backend will expand it relative to the workspace root
      setDirectoryPickerOpen(false);
      setPendingPersona(null);
      setActiveAppId("sessions");
      await createNewChatWithPersona(pendingPersona, path);
    },
    [createNewChatWithPersona, pendingPersona, setActiveAppId],
  );

  const handleDirectoryPickerOpenChange = useCallback((open: boolean) => {
    setDirectoryPickerOpen(open);
    if (!open) {
      setPendingPersona(null);
    }
  }, []);

  // Handle session click - select session and switch to chats view
  const handleSessionClick = (sessionId: string) => {
    setSelectedChatSessionId(sessionId);
    setActiveAppId("sessions");
    setMobileMenuOpen(false);
  };

  // Context menu handlers
  const handlePinSession = useCallback((sessionId: string) => {
    setPinnedSessions((prev) => {
      const next = new Set(prev);
      if (next.has(sessionId)) {
        next.delete(sessionId);
      } else {
        next.add(sessionId);
      }
      return next;
    });
  }, []);

  const handleRenameSession = useCallback((sessionId: string) => {
    // Use chatHistory (disk-based) to find session title
    const session = chatHistory.find((s) => s.id === sessionId);
    setTargetSessionId(sessionId);
    setRenameValue(session?.title || "");
    setRenameDialogOpen(true);
  }, [chatHistory]);

  const handleConfirmRename = useCallback(async () => {
    if (targetSessionId && renameValue.trim()) {
      await renameChatSession(targetSessionId, renameValue.trim());
    }
    setRenameDialogOpen(false);
    setTargetSessionId("");
    setRenameValue("");
  }, [targetSessionId, renameValue, renameChatSession]);

  const handleDeleteSession = useCallback((sessionId: string) => {
    setTargetSessionId(sessionId);
    setDeleteDialogOpen(true);
  }, []);

  const handleConfirmDelete = useCallback(async () => {
    if (targetSessionId) {
      await deleteChatSession(targetSessionId);
    }
    setDeleteDialogOpen(false);
    setTargetSessionId("");
  }, [targetSessionId, deleteChatSession]);

  const handleNewChat = useCallback(async () => {
    console.log("[handleNewChat] called", { 
      selectedWorkspaceSession: !!selectedWorkspaceSession, 
      opencodeBaseUrl,
      selectedProjectKey,
      projectSummaries: projectSummaries.map(p => ({ key: p.key, directory: p.directory }))
    });
    
    // If we have a running workspace session, create a new chat in it
    if (selectedWorkspaceSession && opencodeBaseUrl) {
      console.log("[handleNewChat] Using existing workspace session");
      setActiveAppId("sessions");
      await createNewChat();
      return;
    }
    
    // Check if we have a project filter selected - use that workspace
    if (selectedProjectKey) {
      const project = projectSummaries.find((p) => p.key === selectedProjectKey);
      console.log("[handleNewChat] Project filter selected:", { selectedProjectKey, project });
      if (project?.directory) {
        console.log("[handleNewChat] Starting session for project:", project.directory);
        setActiveAppId("sessions");
        const baseUrl = await ensureOpencodeRunning(project.directory);
        console.log("[handleNewChat] Got baseUrl:", baseUrl);
        if (baseUrl) {
          await createNewChat(baseUrl);
          return;
        }
      }
    }
    
    // Check if we have a workspace path from the current chat history
    // This happens when viewing a historical chat without a running session
    const currentWorkspacePath = selectedChatFromHistory?.workspace_path;
    if (currentWorkspacePath && currentWorkspacePath !== "global") {
      // Start a session for this workspace and create a new chat
      console.log("[handleNewChat] Using workspace from history:", currentWorkspacePath);
      setActiveAppId("sessions");
      const baseUrl = await ensureOpencodeRunning(currentWorkspacePath);
      if (baseUrl) {
        await createNewChat(baseUrl);
        return;
      }
    }
    
    // No workspace context - open persona picker to select one
    console.log("[handleNewChat] Opening agent picker");
    setAgentPickerOpen(true);
  }, [selectedWorkspaceSession, opencodeBaseUrl, selectedChatFromHistory, selectedProjectKey, projectSummaries, ensureOpencodeRunning, createNewChat, setActiveAppId]);

  const handleAgentSelect = useCallback(async (persona: Persona) => {
    if (persona.workspace_mode === "ask") {
      setPendingPersona(persona);
      setDirectoryPickerPath(".");
      setDirectoryPickerOpen(true);
      return;
    }
    setActiveAppId("sessions");
    await createNewChatWithPersona(persona);
  }, [createNewChatWithPersona, setActiveAppId]);

  useEffect(() => {
    if (typeof window === "undefined") return;

    // Top loading bar animation
    setBarVisible(true);
    setBarWidth(25);
    const growTimer = window.setTimeout(() => setBarWidth(80), 150);
    const finish = () => {
      setBarWidth(100);
      setBarFade(true);
      window.setTimeout(() => setBarVisible(false), 500);
    };
    window.addEventListener("load", finish, { once: true });
    const fallback = window.setTimeout(finish, 1600);
    return () => {
      window.clearTimeout(growTimer);
      window.clearTimeout(fallback);
      window.removeEventListener("load", finish);
    };
  }, []);

  const toggleTheme = () => {
    const next = isDark ? "light" : "dark";
    // Disable transitions during theme switch
    document.documentElement.classList.add("no-transitions");
    setTheme(next);
    // Re-enable transitions after a brief delay
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        document.documentElement.classList.remove("no-transitions");
      });
    });
  };

  const toggleLocale = () => {
    const next = locale === "de" ? "en" : "de";
    setLocale(next);
  };

  const shellBg = "var(--background)";
  const sidebarBg = "var(--sidebar, #181b1a)";
  const navIdle = "var(--sidebar, #181b1a)";
  const sidebarHover = "rgba(59, 167, 124, 0.12)";
  const sidebarHoverBorder = "transparent";
  const navText = "var(--sidebar-foreground, #dfe5e1)";
  const navActiveBg = "#3ba77c";
  const navActiveText = "#0b0f0d";
  const navActiveBorder = "#3ba77c";

  const navIconFor = (id: string) => {
    switch (id) {
      case "projects":
        return FolderKanban;
      case "sessions":
        return MessageSquare;
      case "agents":
        return Bot;
      case "admin":
        return Shield;
      default:
        return FolderKanban;
    }
  };

  const sidebarTabs = useMemo(() => {
    const ordered = ["sessions", "projects", "agents"];
    return ordered.map((id) => {
      const app = apps.find((entry) => entry.id === id);
      return {
        id,
        label: app ? resolveText(app.label) : id,
        icon: navIconFor(id),
      };
    });
  }, [apps, resolveText]);

  const handleMobileNavClick = (appId: string) => {
    setActiveAppId(appId);
    setMobileMenuOpen(false);
  };

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden">
      {/* Mobile header */}
      <header
        className="fixed top-0 left-0 right-0 h-14 flex items-center px-3 z-50 md:hidden"
        style={{ backgroundColor: sidebarBg }}
      >
        <Button
          variant="ghost"
          size="icon"
          aria-label="Menu"
          onClick={() => setMobileMenuOpen(true)}
          className="text-muted-foreground hover:text-primary flex-shrink-0"
        >
          <Menu className="w-5 h-5" />
        </Button>
        {/* Session info in center - uses chatHistory (disk-based, no opencode needed) */}
        {selectedChatFromHistory ? (
          <div className="flex-1 min-w-0 px-3 text-center">
            <div className="text-sm font-medium text-foreground truncate">
              {selectedChatFromHistory.title?.replace(/\s*-\s*\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z?$/, "").trim() || "Chat"}
            </div>
            <div className="text-[10px] text-muted-foreground truncate">
              {generateReadableId(selectedChatFromHistory.id)}
              {selectedChatFromHistory.updated_at && (
                <span className="opacity-60"> | {formatSessionDate(selectedChatFromHistory.updated_at)}</span>
              )}
            </div>
          </div>
        ) : (
          <div className="flex-1 flex justify-center">
            <Image
              src={isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"}
              alt="OCTO"
              width={80}
              height={32}
              className="h-8 w-auto object-contain"
              priority
              unoptimized
            />
          </div>
        )}
        {/* New chat button */}
        <Button
          variant="ghost"
          size="icon"
          aria-label={locale === "de" ? "Neuer Chat" : "New Chat"}
          onClick={handleNewChat}
          className="text-muted-foreground hover:text-primary flex-shrink-0"
        >
          <Plus className="w-5 h-5" />
        </Button>
      </header>

      {/* Mobile fullscreen menu */}
      {mobileMenuOpen && (
        <div
          className="fixed inset-0 z-50 flex flex-col md:hidden"
          style={{ backgroundColor: sidebarBg }}
        >
          <div className="h-14 flex items-center px-3">
            <Image
              src={isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"}
              alt="OCTO"
              width={70}
              height={28}
              className="h-7 w-auto object-contain flex-shrink-0"
              priority
              unoptimized
            />
            {/* Nav tabs in center */}
            <div className="flex-1 flex items-center justify-center gap-1 px-2">
              {sidebarTabs.map((tab) => {
                const isActive = activeAppId === tab.id;
                const Icon = tab.icon;
                return (
                  <button
                    key={tab.id}
                    onClick={() => handleMobileNavClick(tab.id)}
                    className="px-3 py-1.5 transition flex items-center gap-1.5"
                    style={{
                      backgroundColor: isActive ? navActiveBg : "transparent",
                      color: isActive ? navActiveText : navText,
                      border: isActive
                        ? `1px solid ${navActiveBorder}`
                        : "1px solid transparent",
                    }}
                  >
                    <Icon className="w-4 h-4 shrink-0" />
                    <span className="text-[10px] font-medium">{tab.label}</span>
                  </button>
                );
              })}
            </div>
            <Button
              variant="ghost"
              size="icon"
              aria-label="Close menu"
              onClick={() => setMobileMenuOpen(false)}
              className="text-muted-foreground hover:text-primary flex-shrink-0"
            >
              <X className="w-5 h-5" />
            </Button>
          </div>

          <nav className="flex-1 w-full px-3 pt-3 overflow-y-auto">
            {activeAppId === "sessions" && chatHistory.length > 0 && (
              <div className="flex-1 min-h-0 flex flex-col">
                <div className="flex items-center justify-between gap-2 px-2 py-1.5">
                  <div className="flex items-center gap-2">
                    <span className="text-xs uppercase tracking-wide text-muted-foreground">
                      {locale === "de" ? "Verlauf" : "History"}
                    </span>
                    <span className="text-xs text-muted-foreground/50">
                      ({filteredSessions.length}{deferredSearch ? `/${chatHistory.length}` : ""})
                    </span>
                  </div>
                  {selectedProjectLabel && (
                    <button
                      onClick={handleProjectClear}
                      className="flex items-center gap-1 text-[10px] text-muted-foreground/70 hover:text-foreground"
                    >
                      <X className="w-3 h-3" />
                      {selectedProjectLabel}
                    </button>
                  )}
                </div>
                {/* Mobile search input with project filter */}
                <div className="relative px-2 mb-2">
                  <Search className="absolute left-5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground pointer-events-none" />
                  <input
                    type="text"
                    placeholder={locale === "de" ? "Suchen..." : "Search..."}
                    value={sessionSearch}
                    onChange={(e) => setSessionSearch(e.target.value)}
                    className="w-full pl-9 pr-16 py-2 text-sm bg-sidebar-accent/50 border border-sidebar-border rounded placeholder:text-muted-foreground/50 focus:outline-none focus:border-primary/50"
                  />
                  <div className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center gap-1">
                    {sessionSearch && (
                      <button
                        onClick={() => setSessionSearch("")}
                        className="p-1 text-muted-foreground hover:text-foreground"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    )}
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <button
                          className={cn(
                            "p-1 transition-colors rounded",
                            selectedProjectKey
                              ? "text-primary hover:text-primary/80"
                              : "text-muted-foreground hover:text-foreground"
                          )}
                          title={locale === "de" ? "Nach Projekt filtern" : "Filter by project"}
                        >
                          <ChevronDown className="w-4 h-4" />
                        </button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-48 max-h-64 overflow-y-auto">
                        <DropdownMenuItem
                          onClick={handleProjectClear}
                          className={cn(!selectedProjectKey && "bg-accent")}
                        >
                          <span className="truncate">{locale === "de" ? "Alle Projekte" : "All projects"}</span>
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        {projectSummaries.map((project) => (
                          <DropdownMenuItem
                            key={project.key}
                            onClick={() => setSelectedProjectKey(project.key)}
                            className={cn(selectedProjectKey === project.key && "bg-accent")}
                          >
                            <FolderKanban className="w-4 h-4 mr-2 flex-shrink-0 text-primary/70" />
                            <span className="truncate">{project.name}</span>
                            <span className="ml-auto text-xs text-muted-foreground">
                              {project.sessionCount}
                            </span>
                          </DropdownMenuItem>
                        ))}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                </div>
                <div className="flex-1 overflow-y-auto space-y-0.5 px-1">
                  {filteredSessions.length === 0 && deferredSearch && (
                    <div className="text-sm text-muted-foreground/50 text-center py-4">
                      {locale === "de" ? "Keine Ergebnisse" : "No results"}
                    </div>
                  )}
                  {filteredSessions.slice(0, 20).map((session) => {
                    const isSelected = selectedChatSessionId === session.id;
                    const children = sessionHierarchy.childSessionsByParent.get(session.id) || [];
                    const hasChildren = children.length > 0;
                    const isExpanded = expandedSessions.has(session.id);
                    const readableId = generateReadableId(session.id);
                    // ChatSession uses updated_at instead of time.updated
                    const formattedDate = session.updated_at
                      ? formatSessionDate(session.updated_at)
                      : null;
                    const projectLabel = projectLabelForSession(session);
                    return (
                      <div key={session.id}>
                        <ContextMenu>
                          <ContextMenuTrigger asChild>
                            <button
                              onClick={() => handleSessionClick(session.id)}
                              className={cn(
                                "w-full px-2 py-2 text-left transition-colors flex items-start gap-1.5",
                                isSelected
                                  ? "bg-primary/15 border border-primary text-foreground"
                                  : "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
                              )}
                            >
                              {hasChildren ? (
                                <span
                                  role="button"
                                  tabIndex={0}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    toggleSessionExpanded(session.id);
                                  }}
                                  onKeyDown={(e) => {
                                    if (e.key === "Enter" || e.key === " ") {
                                      e.stopPropagation();
                                      toggleSessionExpanded(session.id);
                                    }
                                  }}
                                  className="mt-0.5 p-1 hover:bg-muted rounded flex-shrink-0 cursor-pointer"
                                >
                                  {isExpanded ? (
                                    <ChevronDown className="w-4 h-4" />
                                  ) : (
                                    <ChevronRight className="w-4 h-4" />
                                  )}
                                </span>
                              ) : (
                                <MessageSquare className="w-4 h-4 mt-0.5 flex-shrink-0 text-primary/70" />
                              )}
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center gap-1">
                                  {pinnedSessions.has(session.id) && (
                                    <Pin className="w-3 h-3 flex-shrink-0 text-primary/70" />
                                  )}
                                  <span className="text-sm truncate font-medium">
                                    {session.title || "Untitled"}
                                  </span>
                                  {hasChildren && (
                                    <span className="text-xs text-primary/70">
                                      ({children.length})
                                    </span>
                                  )}
                                  {busySessions.has(session.id) && (
                                    <Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
                                  )}
                                </div>
                                {(formattedDate || projectLabel) && (
                                  <div className="text-xs text-muted-foreground/50 mt-0.5">
                                    {projectLabel}
                                    {formattedDate && <span className="opacity-60"> • {formattedDate}</span>}
                                  </div>
                                )}
                              </div>
                            </button>
                          </ContextMenuTrigger>
                          <ContextMenuContent>
                            <ContextMenuItem
                              onClick={() => {
                                navigator.clipboard.writeText(readableId);
                              }}
                            >
                              <Copy className="w-4 h-4 mr-2" />
                              {readableId}
                            </ContextMenuItem>
                            <ContextMenuSeparator />
                            <ContextMenuItem
                              onClick={() => handlePinSession(session.id)}
                            >
                              <Pin className="w-4 h-4 mr-2" />
                              {pinnedSessions.has(session.id) 
                                ? (locale === "de" ? "Lospinnen" : "Unpin")
                                : (locale === "de" ? "Anpinnen" : "Pin")}
                            </ContextMenuItem>
                            <ContextMenuItem
                              onClick={() => handleRenameSession(session.id)}
                            >
                              <Pencil className="w-4 h-4 mr-2" />
                              {locale === "de" ? "Umbenennen" : "Rename"}
                            </ContextMenuItem>
                            <ContextMenuSeparator />
                            <ContextMenuItem
                              variant="destructive"
                              onClick={() => handleDeleteSession(session.id)}
                            >
                              <Trash2 className="w-4 h-4 mr-2" />
                              {locale === "de" ? "Loschen" : "Delete"}
                            </ContextMenuItem>
                          </ContextMenuContent>
                        </ContextMenu>
                        {/* Child sessions (subagents) in mobile */}
                        {hasChildren && isExpanded && (
                          <div className="ml-6 border-l border-muted pl-2 space-y-1 mt-1">
                            {children.map((child) => {
                              const isChildSelected = selectedChatSessionId === child.id;
                              // ChatSession uses updated_at instead of time.updated
                              const childFormattedDate = child.updated_at
                                ? formatSessionDate(child.updated_at)
                                : null;
                              return (
                                <button
                                  key={child.id}
                                  onClick={() => handleSessionClick(child.id)}
                                  className={cn(
                                    "w-full px-2 py-2 text-left transition-colors text-sm",
                                    isChildSelected
                                      ? "bg-primary/15 border border-primary text-foreground"
                                      : "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
                                  )}
                                >
                                  <div className="flex items-center gap-1">
                                    <Bot className="w-3.5 h-3.5 flex-shrink-0 text-primary/70" />
                                    <span className="truncate font-medium">
                                      {child.title || "Subagent"}
                                    </span>
                                    {busySessions.has(child.id) && (
                                      <Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
                                    )}
                                  </div>
                                  {childFormattedDate && (
                                    <div className="text-xs text-muted-foreground/50 mt-0.5 ml-5">
                                      {childFormattedDate}
                                    </div>
                                  )}
                                </button>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {activeAppId === "projects" && (
              <div className="flex-1 min-h-0 flex flex-col">
                <div className="flex items-center justify-between gap-2 px-2 py-1.5">
                  <span className="text-xs uppercase tracking-wide text-muted-foreground">
                    {locale === "de" ? "Projekte" : "Projects"}
                  </span>
                  <span className="text-xs text-muted-foreground/50">({projectSummaries.length})</span>
                </div>
                <div className="flex-1 overflow-y-auto space-y-2 px-1">
                  {projectSummaries.length === 0 ? (
                    <div className="text-sm text-muted-foreground/60 text-center py-6">
                      {locale === "de" ? "Noch keine Projekte" : "No projects yet"}
                    </div>
                  ) : (
                    projectSummaries.map((project) => {
                      const lastActiveLabel = project.lastActive
                        ? formatSessionDate(project.lastActive)
                        : locale === "de"
                          ? "Nie"
                          : "Never";
                      const defaultAgent = projectDefaultAgents[project.key];
                      return (
                        <div
                          key={project.key}
                          className={cn(
                            "border rounded-md overflow-hidden",
                            selectedProjectKey === project.key ? "border-primary" : "border-sidebar-border",
                          )}
                        >
                          <button
                            onClick={() => handleProjectSelect(project.key)}
                            className="w-full px-3 py-2 text-left hover:bg-sidebar-accent transition-colors"
                          >
                            <div className="flex items-center gap-2">
                              <FolderKanban className="w-4 h-4 text-primary/80" />
                              <span className="text-sm font-medium truncate">{project.name}</span>
                            </div>
                            <div className="text-xs text-muted-foreground/60 mt-1">
                              {project.sessionCount} {locale === "de" ? "Chats" : "chats"} · {lastActiveLabel}
                            </div>
                            <div className="text-xs text-muted-foreground/60 mt-0.5">
                              {locale === "de" ? "Standard-Agent" : "Default agent"}: {defaultAgent || "-"}
                            </div>
                          </button>
                          <div className="px-3 pb-2">
                            <select
                              value={defaultAgent || ""}
                              onChange={(e) => handleProjectDefaultAgentChange(project.key, e.target.value)}
                              className="w-full text-xs bg-sidebar-accent/50 border border-sidebar-border rounded px-2 py-1"
                            >
                              <option value="">
                                {locale === "de" ? "Standard-Agent setzen" : "Set default agent"}
                              </option>
                              {availableAgents.map((agent) => (
                                <option key={agent.id} value={agent.id}>
                                  {agent.name || agent.id}
                                </option>
                              ))}
                            </select>
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>
              </div>
            )}

            {activeAppId === "agents" && (
              <div className="flex-1 min-h-0 flex flex-col">
                <div className="flex items-center justify-between gap-2 px-2 py-1.5">
                  <span className="text-xs uppercase tracking-wide text-muted-foreground">
                    {locale === "de" ? "Agenten" : "Agents"}
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setMobileMenuOpen(false)}
                    className="text-xs"
                  >
                    {locale === "de" ? "Erstellen" : "Create"}
                  </Button>
                </div>
                <div className="flex-1 overflow-y-auto space-y-2 px-1">
                  {availableAgents.length === 0 ? (
                    <div className="text-sm text-muted-foreground/60 text-center py-6">
                      {locale === "de" ? "Keine Agenten gefunden" : "No agents found"}
                    </div>
                  ) : (
                    availableAgents.map((agent) => (
                      <div
                        key={agent.id}
                        className="border border-sidebar-border rounded-md px-3 py-2 text-left"
                      >
                        <div className="text-sm font-medium">{agent.name || agent.id}</div>
                        <div className="text-xs text-muted-foreground/60">
                          {agent.model?.providerID
                            ? `${agent.model.providerID}/${agent.model.modelID ?? ""}`
                            : agent.id}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}
          </nav>

          <div className="w-full px-4 pb-2">
            <div className="h-px w-full bg-primary/50 mb-2" />
            <div className="flex items-center justify-center gap-3">
              <Button
                variant="ghost"
                size="icon"
                onClick={() => handleMobileNavClick("admin")}
                aria-label="Admin"
                className={cn(
                  "hover:bg-sidebar-accent",
                  activeAppId === "admin" ? "text-primary" : "text-muted-foreground hover:text-primary"
                )}
              >
                <Shield className="w-5 h-5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => {
                  toggleLocale();
                  setMobileMenuOpen(false);
                }}
                aria-label="Sprache wechseln"
                className="text-muted-foreground hover:text-primary hover:bg-sidebar-accent"
              >
                <Globe2 className="w-5 h-5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => {
                  toggleTheme();
                  setMobileMenuOpen(false);
                }}
                aria-pressed={isDark}
                className="text-muted-foreground hover:text-primary hover:bg-sidebar-accent"
              >
                {isDark ? (
                  <SunMedium className="w-5 h-5" />
                ) : (
                  <MoonStar className="w-5 h-5" />
                )}
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Desktop sidebar */}
      <aside
        className={`fixed inset-y-0 left-0 flex-col transition-all duration-200 z-40 hidden md:flex ${
          sidebarCollapsed
            ? "w-[4.5rem] items-center"
            : "w-[16.25rem] items-center"
        }`}
        style={{ backgroundColor: sidebarBg }}
      >
        <div className="h-20 w-full flex items-center justify-center px-4 relative">
          {!sidebarCollapsed && (
            <Image
              src={isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"}
              alt="OCTO"
              width={200}
              height={60}
              className="h-14 w-auto object-contain"
              priority
              unoptimized
            />
          )}
          <Button
            variant="ghost"
            size="icon"
            aria-label="Sidebar umschalten"
            onClick={() => setSidebarCollapsed((prev) => !prev)}
            className="text-muted-foreground hover:text-primary absolute right-3"
          >
            {sidebarCollapsed ? (
              <PanelRightClose className="w-4 h-4" />
            ) : (
              <PanelLeftClose className="w-4 h-4" />
            )}
          </Button>
        </div>
        <div className={`w-full ${sidebarCollapsed ? "px-2" : "px-3"} pt-4 pb-3`}>
          <div className={cn("grid gap-1", sidebarCollapsed ? "grid-rows-3" : "grid-cols-3")}>
            {sidebarTabs.map((tab) => {
              const isActive = activeAppId === tab.id;
              const Icon = tab.icon;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveAppId(tab.id)}
                  className={cn(
                    "px-2 py-2 text-xs font-medium tracking-wide transition-colors flex items-center gap-2 justify-center",
                    !sidebarCollapsed && "justify-center"
                  )}
                  style={{
                    backgroundColor: isActive ? navActiveBg : navIdle,
                    color: isActive ? navActiveText : navText,
                    border: isActive
                      ? `1px solid ${navActiveBorder}`
                      : "1px solid transparent",
                  }}
                >
                  <Icon className="w-4 h-4 shrink-0" />
                  {!sidebarCollapsed && (
                    <span className="text-[10px] font-semibold uppercase tracking-wide">
                      {tab.label}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>

        {/* New Chat button */}
        {activeAppId === "sessions" && (
          <div className={`w-full ${sidebarCollapsed ? "px-2" : "px-3"} mt-1`}>
            <Button
              variant="outline"
              size="sm"
              onClick={handleNewChat}
              className={cn(
                "w-full text-xs font-medium flex items-center gap-2 transition-colors",
                "border-primary/50 hover:border-primary hover:bg-primary/10",
                sidebarCollapsed ? "justify-center px-2" : "justify-start px-3"
              )}
            >
              <Plus className="w-3.5 h-3.5 shrink-0" />
              {!sidebarCollapsed && (
                <span>{locale === "de" ? "Neuer Chat" : "New Chat"}</span>
              )}
            </Button>
          </div>
        )}

        {/* Session history list - uses chatHistory (disk-based, no opencode needed) */}
        {activeAppId === "sessions" && !sidebarCollapsed && chatHistory.length > 0 && (
          <div className="w-full px-1.5 mt-3 flex-1 min-h-0 flex flex-col">
            <div className="flex items-center justify-between gap-2 py-1.5 px-1 border-t border-sidebar-border">
              <div className="flex items-center gap-2">
                <span className="text-xs uppercase tracking-wide text-muted-foreground">
                  {locale === "de" ? "Verlauf" : "History"}
                </span>
                <span className="text-xs text-muted-foreground/50">
                  ({filteredSessions.length}{deferredSearch ? `/${chatHistory.length}` : ""})
                </span>
              </div>
              {selectedProjectLabel && (
                <button
                  onClick={handleProjectClear}
                  className="flex items-center gap-1 text-[10px] text-muted-foreground/70 hover:text-foreground"
                >
                  <X className="w-3 h-3" />
                  {selectedProjectLabel}
                </button>
              )}
            </div>
            {/* Search input with project filter */}
            <div className="relative mb-2 px-0.5">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground pointer-events-none" />
              <input
                type="text"
                placeholder={locale === "de" ? "Suchen..." : "Search..."}
                value={sessionSearch}
                onChange={(e) => setSessionSearch(e.target.value)}
                className="w-full pl-7 pr-14 py-1.5 text-xs bg-sidebar-accent/50 border border-sidebar-border rounded placeholder:text-muted-foreground/50 focus:outline-none focus:border-primary/50"
              />
              <div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5">
                {sessionSearch && (
                  <button
                    onClick={() => setSessionSearch("")}
                    className="p-1 text-muted-foreground hover:text-foreground"
                  >
                    <X className="w-3 h-3" />
                  </button>
                )}
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <button
                      className={cn(
                        "p-1 transition-colors rounded",
                        selectedProjectKey
                          ? "text-primary hover:text-primary/80"
                          : "text-muted-foreground hover:text-foreground"
                      )}
                      title={locale === "de" ? "Nach Projekt filtern" : "Filter by project"}
                    >
                      <ChevronDown className="w-3.5 h-3.5" />
                    </button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" className="w-48 max-h-64 overflow-y-auto">
                    <DropdownMenuItem
                      onClick={handleProjectClear}
                      className={cn(!selectedProjectKey && "bg-accent")}
                    >
                      <span className="truncate">{locale === "de" ? "Alle Projekte" : "All projects"}</span>
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    {projectSummaries.map((project) => (
                      <DropdownMenuItem
                        key={project.key}
                        onClick={() => setSelectedProjectKey(project.key)}
                        className={cn(selectedProjectKey === project.key && "bg-accent")}
                      >
                        <FolderKanban className="w-3.5 h-3.5 mr-2 flex-shrink-0 text-primary/70" />
                        <span className="truncate">{project.name}</span>
                        <span className="ml-auto text-[10px] text-muted-foreground">
                          {project.sessionCount}
                        </span>
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </div>
            <div className="flex-1 overflow-y-auto space-y-0.5">
              {filteredSessions.length === 0 && deferredSearch && (
                <div className="text-xs text-muted-foreground/50 text-center py-4">
                  {locale === "de" ? "Keine Ergebnisse" : "No results"}
                </div>
              )}
              {filteredSessions.slice(0, 20).map((session) => {
                const isSelected = selectedChatSessionId === session.id;
                const children = sessionHierarchy.childSessionsByParent.get(session.id) || [];
                const hasChildren = children.length > 0;
                const isExpanded = expandedSessions.has(session.id);
                const readableId = generateReadableId(session.id);
                // ChatSession uses updated_at instead of time.updated
                const formattedDate = session.updated_at 
                  ? formatSessionDate(session.updated_at)
                  : null;
                const projectLabel = projectLabelForSession(session);
                return (
                  <div key={session.id}>
                    <ContextMenu>
                      <ContextMenuTrigger asChild>
                        <button
                          onClick={() => handleSessionClick(session.id)}
                          className={cn(
                            "w-full px-2 py-1.5 text-left transition-colors flex items-start gap-1.5",
                            isSelected
                              ? "bg-primary/15 border border-primary text-foreground"
                              : "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
                          )}
                        >
                          {hasChildren ? (
                            <span
                              role="button"
                              tabIndex={0}
                              onClick={(e) => {
                                e.stopPropagation();
                                toggleSessionExpanded(session.id);
                              }}
                              onKeyDown={(e) => {
                                if (e.key === "Enter" || e.key === " ") {
                                  e.stopPropagation();
                                  toggleSessionExpanded(session.id);
                                }
                              }}
                              className="mt-0.5 p-0.5 hover:bg-muted rounded flex-shrink-0 cursor-pointer"
                            >
                              {isExpanded ? (
                                <ChevronDown className="w-3 h-3" />
                              ) : (
                                <ChevronRight className="w-3 h-3" />
                              )}
                            </span>
                          ) : (
                            <MessageSquare className="w-3.5 h-3.5 mt-0.5 flex-shrink-0 text-primary/70" />
                          )}
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-1">
                              {pinnedSessions.has(session.id) && (
                                <Pin className="w-3 h-3 flex-shrink-0 text-primary/70" />
                              )}
                              <span className="text-sm truncate font-medium">
                                {session.title || "Untitled"}
                              </span>
                              {hasChildren && (
                                <span className="text-[10px] text-primary/70">
                                  ({children.length})
                                </span>
                              )}
                              {busySessions.has(session.id) && (
                                <Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
                              )}
                            </div>
                            {(formattedDate || projectLabel) && (
                              <div className="text-[10px] text-foreground/50 dark:text-muted-foreground mt-0.5">
                                {projectLabel}
                                {formattedDate && <span className="opacity-60"> • {formattedDate}</span>}
                              </div>
                            )}
                          </div>
                        </button>
                      </ContextMenuTrigger>
                      <ContextMenuContent>
                        <ContextMenuItem
                          onClick={() => {
                            navigator.clipboard.writeText(readableId);
                          }}
                        >
                          <Copy className="w-4 h-4 mr-2" />
                          {readableId}
                        </ContextMenuItem>
                        <ContextMenuItem
                          onClick={() => {
                            navigator.clipboard.writeText(session.id);
                          }}
                        >
                          <Copy className="w-4 h-4 mr-2" />
                          {session.id.slice(0, 16)}...
                        </ContextMenuItem>
                        <ContextMenuSeparator />
                        <ContextMenuItem
                          onClick={() => handlePinSession(session.id)}
                        >
                          <Pin className="w-4 h-4 mr-2" />
                          {pinnedSessions.has(session.id) 
                            ? (locale === "de" ? "Lospinnen" : "Unpin")
                            : (locale === "de" ? "Anpinnen" : "Pin")}
                        </ContextMenuItem>
                        <ContextMenuItem
                          onClick={() => handleRenameSession(session.id)}
                        >
                          <Pencil className="w-4 h-4 mr-2" />
                          {locale === "de" ? "Umbenennen" : "Rename"}
                        </ContextMenuItem>
                        <ContextMenuSeparator />
                        <ContextMenuItem
                          variant="destructive"
                          onClick={() => handleDeleteSession(session.id)}
                        >
                          <Trash2 className="w-4 h-4 mr-2" />
                          {locale === "de" ? "Loschen" : "Delete"}
                        </ContextMenuItem>
                      </ContextMenuContent>
                    </ContextMenu>
                    {/* Child sessions (subagents) */}
                    {hasChildren && isExpanded && (
                      <div className="ml-4 border-l border-muted pl-2 space-y-1 mt-1">
                        {children.map((child) => {
                          const isChildSelected = selectedChatSessionId === child.id;
                          const childReadableId = generateReadableId(child.id);
                          // ChatSession uses updated_at instead of time.updated
                          const childFormattedDate = child.updated_at
                            ? formatSessionDate(child.updated_at)
                            : null;
                          return (
                            <ContextMenu key={child.id}>
                              <ContextMenuTrigger asChild>
                                <button
                                  onClick={() => handleSessionClick(child.id)}
                                  className={cn(
                                    "w-full px-2 py-1.5 text-left transition-colors text-xs",
                                    isChildSelected
                                      ? "bg-primary/15 border border-primary text-foreground"
                                      : "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
                                  )}
                                >
                                  <div className="flex items-center gap-1">
                                    <Bot className="w-3 h-3 flex-shrink-0 text-primary/70" />
                                    <span className="truncate font-medium">
                                      {child.title || "Subagent"}
                                    </span>
                                    {busySessions.has(child.id) && (
                                      <Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
                                    )}
                                  </div>
                                  {childFormattedDate && (
                                    <div className="text-[9px] text-foreground/50 dark:text-muted-foreground mt-0.5 ml-4">
                                      {childFormattedDate}
                                    </div>
                                  )}
                                </button>
                              </ContextMenuTrigger>
                              <ContextMenuContent>
                                <ContextMenuItem
                                  onClick={() => {
                                    navigator.clipboard.writeText(childReadableId);
                                  }}
                                >
                                  <Copy className="w-4 h-4 mr-2" />
                                  {childReadableId}
                                </ContextMenuItem>
                                <ContextMenuItem
                                  onClick={() => {
                                    navigator.clipboard.writeText(child.id);
                                  }}
                                >
                                  <Copy className="w-4 h-4 mr-2" />
                                  {child.id.slice(0, 16)}...
                                </ContextMenuItem>
                                <ContextMenuSeparator />
                                <ContextMenuItem
                                  onClick={() => handleRenameSession(child.id)}
                                >
                                  <Pencil className="w-4 h-4 mr-2" />
                                  {locale === "de" ? "Umbenennen" : "Rename"}
                                </ContextMenuItem>
                                <ContextMenuSeparator />
                                <ContextMenuItem
                                  variant="destructive"
                                  onClick={() => handleDeleteSession(child.id)}
                                >
                                  <Trash2 className="w-4 h-4 mr-2" />
                                  {locale === "de" ? "Loschen" : "Delete"}
                                </ContextMenuItem>
                              </ContextMenuContent>
                            </ContextMenu>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {activeAppId === "projects" && !sidebarCollapsed && (
          <div className="w-full px-2 mt-3 flex-1 min-h-0 flex flex-col">
            <div className="flex items-center justify-between gap-2 py-1.5 px-1 border-t border-sidebar-border">
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                {locale === "de" ? "Projekte" : "Projects"}
              </span>
              <span className="text-xs text-muted-foreground/50">({projectSummaries.length})</span>
            </div>
            <div className="flex-1 overflow-y-auto space-y-2 px-1">
              {projectSummaries.length === 0 ? (
                <div className="text-xs text-muted-foreground/60 text-center py-4">
                  {locale === "de" ? "Noch keine Projekte" : "No projects yet"}
                </div>
              ) : (
                projectSummaries.map((project) => {
                  const lastActiveLabel = project.lastActive
                    ? formatSessionDate(project.lastActive)
                    : locale === "de"
                      ? "Nie"
                      : "Never";
                  const defaultAgent = projectDefaultAgents[project.key];
                  return (
                    <div
                      key={project.key}
                      className={cn(
                        "border rounded-md overflow-hidden",
                        selectedProjectKey === project.key ? "border-primary" : "border-sidebar-border",
                      )}
                    >
                      <button
                        onClick={() => handleProjectSelect(project.key)}
                        className="w-full px-3 py-2 text-left hover:bg-sidebar-accent transition-colors"
                      >
                        <div className="flex items-center gap-2">
                          <FolderKanban className="w-4 h-4 text-primary/80" />
                          <span className="text-sm font-medium truncate">{project.name}</span>
                        </div>
                        <div className="text-xs text-muted-foreground/60 mt-1">
                          {project.sessionCount} {locale === "de" ? "Chats" : "chats"} · {lastActiveLabel}
                        </div>
                        <div className="text-xs text-muted-foreground/60 mt-0.5">
                          {locale === "de" ? "Standard-Agent" : "Default agent"}: {defaultAgent || "-"}
                        </div>
                      </button>
                      <div className="px-3 pb-2">
                        <select
                          value={defaultAgent || ""}
                          onChange={(e) => handleProjectDefaultAgentChange(project.key, e.target.value)}
                          className="w-full text-xs bg-sidebar-accent/50 border border-sidebar-border rounded px-2 py-1"
                        >
                          <option value="">
                            {locale === "de" ? "Standard-Agent setzen" : "Set default agent"}
                          </option>
                          {availableAgents.map((agent) => (
                            <option key={agent.id} value={agent.id}>
                              {agent.name || agent.id}
                            </option>
                          ))}
                        </select>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        )}

        {activeAppId === "agents" && !sidebarCollapsed && (
          <div className="w-full px-2 mt-3 flex-1 min-h-0 flex flex-col">
            <div className="flex items-center justify-between gap-2 py-1.5 px-1 border-t border-sidebar-border">
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                {locale === "de" ? "Agenten" : "Agents"}
              </span>
              <Button variant="ghost" size="sm" className="text-xs" onClick={() => setActiveAppId("agents")}>
                {locale === "de" ? "Erstellen" : "Create"}
              </Button>
            </div>
            <div className="flex-1 overflow-y-auto space-y-2 px-1">
              {availableAgents.length === 0 ? (
                <div className="text-xs text-muted-foreground/60 text-center py-4">
                  {locale === "de" ? "Keine Agenten gefunden" : "No agents found"}
                </div>
              ) : (
                availableAgents.map((agent) => (
                  <div
                    key={agent.id}
                    className="border border-sidebar-border rounded-md px-3 py-2 text-left"
                  >
                    <div className="text-sm font-medium">{agent.name || agent.id}</div>
                    <div className="text-xs text-muted-foreground/60">
                      {agent.model?.providerID
                        ? `${agent.model.providerID}/${agent.model.modelID ?? ""}`
                        : agent.id}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {/* Collapsed session indicator */}
        {activeAppId === "sessions" && sidebarCollapsed && opencodeSessions.length > 0 && (
          <div className="w-full px-2 mt-4">
            <div className="border-t border-sidebar-border pt-2">
              <button
                onClick={() => setSidebarCollapsed(false)}
                className="w-full p-2 text-muted-foreground hover:text-foreground transition-colors"
                title={locale === "de" ? "Verlauf anzeigen" : "Show history"}
              >
                <Clock className="w-4 h-4 mx-auto" />
              </button>
            </div>
          </div>
        )}

        <div
          className={`w-full ${sidebarCollapsed ? "px-2 pb-3" : "px-4 pb-4"} mt-auto pt-3`}
        >
          <div className="h-px w-full bg-primary/50 mb-3" />
          <div className={`flex items-center ${sidebarCollapsed ? "flex-col gap-2" : "justify-center gap-2"}`}>
            <Button
              variant="ghost"
              size="default"
              onClick={() => setActiveAppId("admin")}
              aria-label="Admin"
              className={`${sidebarCollapsed ? "w-full" : ""} px-3 py-2 text-sm font-medium flex items-center justify-center transition-colors`}
              style={{
                backgroundColor: activeAppId === "admin" ? navActiveBg : navIdle,
                border: activeAppId === "admin" ? `1px solid ${navActiveBorder}` : "1px solid transparent",
                color: activeAppId === "admin" ? navActiveText : navText,
              }}
              onMouseEnter={(e) => {
                if (activeAppId !== "admin") {
                  e.currentTarget.style.backgroundColor = sidebarHover;
                  e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
                }
              }}
              onMouseLeave={(e) => {
                if (activeAppId !== "admin") {
                  e.currentTarget.style.backgroundColor = navIdle;
                  e.currentTarget.style.border = "1px solid transparent";
                }
              }}
            >
              <Shield className="w-4 h-4" />
            </Button>
            <Button
              variant="ghost"
              size="default"
              onClick={toggleLocale}
              aria-label="Sprache wechseln"
              className={`${sidebarCollapsed ? "w-full" : ""} px-3 py-2 text-sm font-medium flex items-center justify-center transition-colors`}
              style={{
                backgroundColor: navIdle,
                border: "1px solid transparent",
                color: navText,
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.backgroundColor = sidebarHover;
                e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.backgroundColor = navIdle;
                e.currentTarget.style.border = "1px solid transparent";
              }}
            >
              <Globe2 className="w-4 h-4" />
            </Button>
            <Button
              variant="ghost"
              size="default"
              onClick={toggleTheme}
              aria-pressed={isDark}
              className={`${sidebarCollapsed ? "w-full" : ""} px-3 py-2 text-sm font-medium flex items-center justify-center transition-colors`}
              style={{
                backgroundColor: navIdle,
                border: "1px solid transparent",
                color: navText,
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.backgroundColor = sidebarHover;
                e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.backgroundColor = navIdle;
                e.currentTarget.style.border = "1px solid transparent";
              }}
            >
              {isDark ? (
                <SunMedium className="w-4 h-4" />
              ) : (
                <MoonStar className="w-4 h-4" />
              )}
            </Button>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <div
        className="flex-1 flex flex-col min-h-0 overflow-hidden"
        style={{ backgroundColor: shellBg }}
      >
        <div
          className={`flex-1 min-h-0 overflow-hidden pt-14 md:pt-0 transition-all duration-200 ${
            sidebarCollapsed ? "md:pl-[4.5rem]" : "md:pl-[16.25rem]"
          }`}
        >
          <div className="h-full w-full">
            {ActiveComponent ? <ActiveComponent /> : <EmptyState />}
          </div>
        </div>
      </div>

      {/* Loading bar */}
      {barVisible && (
        <div className="fixed left-0 top-0 z-[100] w-full pointer-events-none">
          <div
            style={{
              height: "2px",
              width: `${barWidth}%`,
              maxWidth: "100%",
              backgroundColor: "var(--sidebar-ring, #3ba77c)",
              opacity: barFade ? 0 : 1,
              boxShadow: "0 0 12px rgba(59,167,124,0.6)",
              transition: "width 320ms ease, opacity 450ms ease",
            }}
          />
        </div>
      )}

      {/* Command palette */}
      <CommandPalette open={commandPaletteOpen} onOpenChange={setCommandPaletteOpen} />

      {/* Delete confirmation dialog */}
      <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {locale === "de" ? "Chat loschen?" : "Delete chat?"}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {locale === "de"
                ? "Diese Aktion kann nicht ruckgangig gemacht werden. Der Chat wird dauerhaft geloscht."
                : "This action cannot be undone. The chat will be permanently deleted."}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              {locale === "de" ? "Abbrechen" : "Cancel"}
            </AlertDialogCancel>
            <AlertDialogAction onClick={handleConfirmDelete}>
              {locale === "de" ? "Loschen" : "Delete"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Rename dialog */}
      <Dialog open={renameDialogOpen} onOpenChange={setRenameDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {locale === "de" ? "Chat umbenennen" : "Rename chat"}
            </DialogTitle>
            <DialogDescription>
              {locale === "de"
                ? "Geben Sie einen neuen Namen fur diesen Chat ein."
                : "Enter a new name for this chat."}
            </DialogDescription>
          </DialogHeader>
          <Input
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            placeholder={locale === "de" ? "Chat-Titel" : "Chat title"}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                handleConfirmRename();
              }
            }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameDialogOpen(false)}>
              {locale === "de" ? "Abbrechen" : "Cancel"}
            </Button>
            <Button onClick={handleConfirmRename}>
              {locale === "de" ? "Speichern" : "Save"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Directory picker dialog */}
      <Dialog open={directoryPickerOpen} onOpenChange={handleDirectoryPickerOpenChange}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{locale === "de" ? "Arbeitsordner wahlen" : "Choose workspace folder"}</DialogTitle>
            <DialogDescription>
              {locale === "de"
                ? "Wahle ein Projektverzeichnis fur diesen Chat."
                : "Pick a project directory for this chat."}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{directoryPickerPath}</span>
              {directoryPickerParent && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setDirectoryPickerPath(directoryPickerParent)}
                >
                  {locale === "de" ? "Hoch" : "Up"}
                </Button>
              )}
            </div>

            {recentProjects.length > 0 && (
              <div className="space-y-2">
                <div className="text-xs uppercase text-muted-foreground">
                  {locale === "de" ? "Zuletzt verwendet" : "Recent projects"}
                </div>
                <div className="grid gap-2 sm:grid-cols-2">
                  {recentProjects.map((project) => (
                    <button
                      key={project.key}
                      onClick={() => handleDirectoryConfirm(project.key === "workspace" ? "." : project.key)}
                      className="text-left border border-border rounded px-3 py-2 hover:bg-muted transition-colors"
                    >
                      <div className="text-sm font-medium truncate">{project.name}</div>
                      <div className="text-xs text-muted-foreground truncate">{project.key}</div>
                    </button>
                  ))}
                </div>
              </div>
            )}

            <div className="space-y-2">
              <div className="text-xs uppercase text-muted-foreground">
                {locale === "de" ? "Ordner" : "Folders"}
              </div>
              <div className="max-h-56 overflow-y-auto border border-border rounded">
                {directoryPickerLoading ? (
                  <div className="p-4 text-sm text-muted-foreground">
                    {locale === "de" ? "Lade..." : "Loading..."}
                  </div>
                ) : directoryPickerEntries.length === 0 ? (
                  <div className="p-4 text-sm text-muted-foreground">
                    {locale === "de" ? "Keine Ordner gefunden" : "No folders found"}
                  </div>
                ) : (
                  <div className="divide-y divide-border">
                    {directoryPickerEntries.map((entry) => (
                      <div key={entry.path} className="flex items-center justify-between px-3 py-2">
                        <button
                          onClick={() => setDirectoryPickerPath(entry.path)}
                          className="text-sm text-left flex-1 truncate hover:text-foreground"
                        >
                          {entry.name}
                        </button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDirectoryConfirm(entry.path)}
                        >
                          {locale === "de" ? "Wahlen" : "Select"}
                        </Button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDirectoryPickerOpen(false)}
            >
              {locale === "de" ? "Abbrechen" : "Cancel"}
            </Button>
            <Button onClick={() => handleDirectoryConfirm(directoryPickerPath)}>
              {locale === "de" ? "Diesen Ordner nutzen" : "Use this folder"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Agent picker dialog */}
      <AgentPicker
        open={agentPickerOpen}
        onOpenChange={setAgentPickerOpen}
        onSelect={handleAgentSelect}
        locale={locale}
      />
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex items-center justify-center h-full">
      <div className="text-center space-y-2">
        <p className="text-sm text-muted-foreground">No apps registered</p>
        <p className="text-xs text-muted-foreground">
          Register an app in apps/index.ts to get started.
        </p>
      </div>
    </div>
  );
}

export default function AgentWorkspacePlatform() {
  return (
    <AppProvider>
      <AppShell />
    </AppProvider>
  );
}
