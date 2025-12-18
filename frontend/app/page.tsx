"use client";

import { useEffect, useRef, useState, useCallback, useMemo, useDeferredValue } from "react";
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
  Snowflake,
  Pin,
  Pencil,
  Trash2,
  Plus,
  ChevronRight,
  ChevronDown,
  Copy,
  Search,
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
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { CommandPalette, useCommandPalette } from "@/components/command-palette";
import { generateReadableId, formatSessionDate } from "@/lib/session-utils";
import "@/apps";

function SnowOverlay({ intensity = 0.5 }: { intensity?: number }) {
  const clamped = Math.max(0, Math.min(1, intensity));
  const count = Math.round(90 + 150 * clamped);
  const speedFactor = 1.05 - clamped * 0.35;
  const flakes = Array.from({ length: count });
  return (
    <>
      <div className="pointer-events-none fixed inset-0 z-50 overflow-hidden">
        {flakes.map((_, i) => {
          const size = Math.random() * 3 + 2 + clamped;
          const left = Math.random() * 100;
          const duration = (Math.random() * 6 + 5) * speedFactor;
          const delay = Math.random() * 6;
          return (
            <span
              key={i}
              className="absolute rounded-full bg-white opacity-70 animate-snowfall"
              style={{
                width: size,
                height: size,
                left: `${left}%`,
                animationDuration: `${duration}s`,
                animationDelay: `${delay}s`,
              }}
            />
          );
        })}
      </div>
      <style jsx global>{`
        @keyframes snowfall {
          0% {
            transform: translateY(-10%);
            opacity: 0.9;
          }
          100% {
            transform: translateY(110vh);
            opacity: 0.1;
          }
        }
        .animate-snowfall {
          animation-name: snowfall;
          animation-timing-function: linear;
          animation-iteration-count: infinite;
        }
      `}</style>
    </>
  );
}

function AppShell() {
  const {
    apps,
    activeAppId,
    setActiveAppId,
    activeApp,
    locale,
    setLocale,
    resolveText,
    opencodeSessions,
    selectedChatSessionId,
    setSelectedChatSessionId,
    createNewChat,
    deleteChatSession,
    renameChatSession,
  } = useApp();
  const { theme, setTheme, resolvedTheme } = useTheme();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const ActiveComponent = activeApp?.component ?? null;

  // JAK Christmas feature
  const [showJakSnow, setShowJakSnow] = useState(false);
  const jakAudioRef = useRef<HTMLAudioElement | null>(null);
  const [jakVolume, setJakVolume] = useState(0.5);

  // Loading bar
  const [barVisible, setBarVisible] = useState(true);
  const [barWidth, setBarWidth] = useState(0);
  const [barFade, setBarFade] = useState(false);

  // Dialog states
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [renameDialogOpen, setRenameDialogOpen] = useState(false);
  const [targetSessionId, setTargetSessionId] = useState<string>("");
  const [renameValue, setRenameValue] = useState("");

  // Command palette
  const { open: commandPaletteOpen, setOpen: setCommandPaletteOpen } = useCommandPalette();

  // Expanded state for parent sessions in sidebar
  const [expandedSessions, setExpandedSessions] = useState<Set<string>>(new Set());

  // Session search
  const [sessionSearch, setSessionSearch] = useState("");
  const deferredSearch = useDeferredValue(sessionSearch);

  // Build hierarchical session structure
  const sessionHierarchy = useMemo(() => {
    // Separate parent and child sessions
    const parentSessions = opencodeSessions.filter((s) => !s.parentID);
    const childSessionsByParent = new Map<string, typeof opencodeSessions>();
    
    for (const session of opencodeSessions) {
      if (session.parentID) {
        const children = childSessionsByParent.get(session.parentID) || [];
        children.push(session);
        childSessionsByParent.set(session.parentID, children);
      }
    }
    
    // Sort children by updated time (newest first)
    for (const [parentId, children] of childSessionsByParent) {
      childSessionsByParent.set(
        parentId,
        children.sort((a, b) => b.time.updated - a.time.updated)
      );
    }
    
    return { parentSessions, childSessionsByParent };
  }, [opencodeSessions]);

  // Filter sessions based on search term
  const filteredSessions = useMemo(() => {
    const searchLower = deferredSearch.toLowerCase().trim();
    if (!searchLower) {
      return sessionHierarchy.parentSessions;
    }
    
    return sessionHierarchy.parentSessions.filter((session) => {
      // Search in title
      if (session.title?.toLowerCase().includes(searchLower)) return true;
      // Search in readable ID (adjective-noun)
      const readableId = generateReadableId(session.id);
      if (readableId.toLowerCase().includes(searchLower)) return true;
      // Search in date
      if (session.time?.updated) {
        const dateStr = formatSessionDate(session.time.updated);
        if (dateStr.toLowerCase().includes(searchLower)) return true;
      }
      return false;
    });
  }, [sessionHierarchy.parentSessions, deferredSearch]);

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

  // Handle session click - select session and switch to chats view
  const handleSessionClick = (sessionId: string) => {
    setSelectedChatSessionId(sessionId);
    setActiveAppId("sessions");
    setMobileMenuOpen(false);
  };

  // Context menu handlers
  const handlePinSession = (sessionId: string) => {
    console.log("Pin session:", sessionId);
    // TODO: Implement pin functionality - requires backend support
  };

  const handleRenameSession = useCallback((sessionId: string) => {
    const session = opencodeSessions.find((s) => s.id === sessionId);
    setTargetSessionId(sessionId);
    setRenameValue(session?.title || "");
    setRenameDialogOpen(true);
  }, [opencodeSessions]);

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

  useEffect(() => {
    if (typeof window === "undefined") return;

    // Load saved JAK volume
    const storedVol = localStorage.getItem("jakVolume");
    const volNum = storedVol ? Number(storedVol) : NaN;
    if (!Number.isNaN(volNum) && volNum >= 0 && volNum <= 1) {
      setJakVolume(volNum);
      if (jakAudioRef.current) {
        jakAudioRef.current.volume = volNum;
      }
    }

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
    const next = resolvedTheme === "dark" ? "light" : "dark";
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

  const toggleJakSnow = () => {
    setShowJakSnow((prev) => {
      const next = !prev;
      let audio = jakAudioRef.current;
      if (!audio) {
        audio = new Audio("/audio/jingle-bells.mp3");
        audio.loop = true;
        audio.preload = "none";
        audio.volume = jakVolume;
        // Silently handle errors (file may not exist)
        audio.onerror = () => {};
        jakAudioRef.current = audio;
      }
      if (audio) {
        if (next) {
          audio.currentTime = 0;
          audio.muted = false;
          audio.volume = jakVolume;
          audio
            .play()
            .then(() => {
              // playback started
            })
            .catch(() => {
              // Audio playback blocked or file not found - fail silently
            });
        } else {
          audio.pause();
          audio.currentTime = 0;
        }
      }
      return next;
    });
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
      case "workspaces":
        return Bot;
      case "admin":
        return Shield;
      default:
        return FolderKanban;
    }
  };

  const handleMobileNavClick = (appId: string) => {
    setActiveAppId(appId);
    setMobileMenuOpen(false);
  };

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden">
      {/* Mobile header */}
      <header
        className="fixed top-0 left-0 right-0 h-14 flex items-center justify-between px-4 z-50 md:hidden"
        style={{ backgroundColor: sidebarBg }}
      >
        <Button
          variant="ghost"
          size="icon"
          aria-label="Menu"
          onClick={() => setMobileMenuOpen(true)}
          className="text-muted-foreground hover:text-primary"
        >
          <Menu className="w-5 h-5" />
        </Button>
        <Image
          src={resolvedTheme === "dark" ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"}
          alt="OCTO"
          width={80}
          height={32}
          className="h-8 w-auto object-contain"
          priority
          unoptimized
        />
      </header>

      {/* Mobile fullscreen menu */}
      {mobileMenuOpen && (
        <div
          className="fixed inset-0 z-50 flex flex-col md:hidden"
          style={{ backgroundColor: sidebarBg }}
        >
          <div className="h-14 flex items-center justify-between px-4">
            <Image
              src={resolvedTheme === "dark" ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"}
              alt="OCTO"
              width={80}
              height={32}
              className="h-8 w-auto object-contain"
              priority
              unoptimized
            />
            <Button
              variant="ghost"
              size="icon"
              aria-label="Close menu"
              onClick={() => setMobileMenuOpen(false)}
              className="text-muted-foreground hover:text-primary"
            >
              <X className="w-5 h-5" />
            </Button>
          </div>

          <nav className="flex-1 w-full space-y-3 px-4 pt-6 overflow-y-auto">
            {apps.map((app) => {
              const isActive = activeAppId === app.id;
              const Icon = navIconFor(app.id);
              return (
                <button
                  key={app.id}
                  onClick={() => handleMobileNavClick(app.id)}
                  className="w-full rounded-none px-4 py-4 text-base font-semibold tracking-wide transition flex items-center gap-3"
                  style={{
                    backgroundColor: isActive ? navActiveBg : navIdle,
                    color: isActive ? navActiveText : navText,
                    border: isActive
                      ? `1px solid ${navActiveBorder}`
                      : "1px solid transparent",
                  }}
                >
                  <Icon className="w-5 h-5 shrink-0" />
                  <span>{resolveText(app.label)}</span>
                </button>
              );
            })}

            {/* Session history in mobile menu */}
            {opencodeSessions.length > 0 && (
              <div className="pt-4 border-t border-sidebar-border">
                <div className="flex items-center gap-2 px-4 py-2">
                  <span className="text-xs uppercase tracking-wide text-muted-foreground">
                    {locale === "de" ? "Verlauf" : "History"}
                  </span>
                  <span className="text-xs text-muted-foreground/50">
                    ({opencodeSessions.length})
                  </span>
                </div>
                <div className="space-y-1">
                  {opencodeSessions.slice(0, 10).map((session) => {
                    const isSelected = selectedChatSessionId === session.id;
                    const updatedAt = session.time?.updated
                      ? new Date(session.time.updated)
                      : null;
                    return (
                      <ContextMenu key={session.id}>
                        <ContextMenuTrigger asChild>
                          <button
                            onClick={() => handleSessionClick(session.id)}
                            className={cn(
                              "w-full px-4 py-3 text-left transition-colors",
                              isSelected
                                ? "bg-primary/15 text-foreground"
                                : "text-muted-foreground hover:bg-sidebar-accent",
                            )}
                          >
                            <div className="text-sm truncate">
                              {session.title || "Untitled"}
                            </div>
                            <div className="text-xs text-muted-foreground/50 truncate">
                              {session.id.slice(0, 12)}
                            </div>
                          </button>
                        </ContextMenuTrigger>
                        <ContextMenuContent>
                          <ContextMenuItem
                            onClick={() => handlePinSession(session.id)}
                          >
                            <Pin className="w-4 h-4 mr-2" />
                            {locale === "de" ? "Anpinnen" : "Pin"}
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
                    );
                  })}
                </div>
              </div>
            )}
          </nav>

          <div className="w-full px-4 pb-8 space-y-3">
            <div className="h-px w-full bg-primary/50 mt-2" />
            <Button
              variant="ghost"
              size="lg"
              onClick={() => {
                toggleLocale();
                setMobileMenuOpen(false);
              }}
              aria-label="Sprache wechseln"
              className="w-full justify-start text-muted-foreground hover:text-primary py-4"
            >
              <Globe2 className="w-5 h-5" />
              <span className="text-base font-semibold">
                {locale === "de" ? "EN" : "DE"}
              </span>
            </Button>
            <Button
              variant="ghost"
              size="lg"
              onClick={() => {
                toggleTheme();
                setMobileMenuOpen(false);
              }}
              aria-pressed={resolvedTheme === "dark"}
              className="w-full justify-start text-muted-foreground hover:text-primary py-4"
            >
              {resolvedTheme === "dark" ? (
                <SunMedium className="w-5 h-5" />
              ) : (
                <MoonStar className="w-5 h-5" />
              )}
              <span className="text-base font-semibold">Theme</span>
            </Button>
            <Button
              variant="ghost"
              size="lg"
              onClick={() => {
                toggleJakSnow();
                setMobileMenuOpen(false);
              }}
              className="w-full justify-start text-muted-foreground hover:text-primary py-4"
            >
              <Snowflake className="w-5 h-5" />
              <span className="text-base font-semibold">JAK</span>
            </Button>
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
        <div className="h-24 w-full flex items-center justify-center px-4 relative">
          {!sidebarCollapsed && (
            <Image
              src={resolvedTheme === "dark" ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"}
              alt="OCTO"
              width={240}
              height={80}
              className="h-20 w-auto object-contain"
              priority
              unoptimized
            />
          )}
          <Button
            variant="ghost"
            size="icon"
            aria-label="Sidebar umschalten"
            onClick={() => setSidebarCollapsed((prev) => !prev)}
            className="text-muted-foreground hover:text-primary absolute right-4"
          >
            {sidebarCollapsed ? (
              <PanelRightClose className="w-4 h-4" />
            ) : (
              <PanelLeftClose className="w-4 h-4" />
            )}
          </Button>
        </div>
        <nav
          className={`w-full space-y-3 ${sidebarCollapsed ? "px-2" : "px-4"} pt-6 pb-4`}
        >
          {apps.map((app) => {
            const isActive = activeAppId === app.id;
            const Icon = navIconFor(app.id);
            return (
              <button
                key={app.id}
                onClick={() => setActiveAppId(app.id)}
                className={`w-full px-4 py-3 text-sm font-medium tracking-wide transition-colors flex items-center gap-2 ${
                  sidebarCollapsed ? "justify-center" : ""
                }`}
                style={{
                  backgroundColor: isActive ? navActiveBg : navIdle,
                  color: isActive ? navActiveText : navText,
                  border: isActive
                    ? `1px solid ${navActiveBorder}`
                    : "1px solid transparent",
                }}
                onMouseEnter={(e) => {
                  if (!isActive) {
                    e.currentTarget.style.backgroundColor = sidebarHover;
                    e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
                  }
                }}
                onMouseLeave={(e) => {
                  if (!isActive) {
                    e.currentTarget.style.backgroundColor = navIdle;
                    e.currentTarget.style.border = "1px solid transparent";
                  }
                }}
              >
                <Icon className="w-4 h-4 shrink-0" />
                {!sidebarCollapsed && (
                  <span className="truncate">{resolveText(app.label)}</span>
                )}
              </button>
            );
          })}
        </nav>

        {/* New Chat button */}
        <div className={`w-full ${sidebarCollapsed ? "px-2" : "px-4"} mt-2`}>
          <Button
            variant="outline"
            size="default"
            onClick={() => void createNewChat()}
            className={cn(
              "w-full text-sm font-medium flex items-center gap-2 transition-colors",
              "border-primary/50 hover:border-primary hover:bg-primary/10",
              sidebarCollapsed ? "justify-center px-2" : "justify-start px-4"
            )}
          >
            <Plus className="w-4 h-4 shrink-0" />
            {!sidebarCollapsed && (
              <span>{locale === "de" ? "Neuer Chat" : "New Chat"}</span>
            )}
          </Button>
        </div>

        {/* Session history list */}
        {!sidebarCollapsed && opencodeSessions.length > 0 && (
          <div className="w-full px-4 mt-4 flex-1 min-h-0 flex flex-col">
            <div className="flex items-center gap-2 py-2 border-t border-sidebar-border">
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                {locale === "de" ? "Verlauf" : "History"}
              </span>
              <span className="text-xs text-muted-foreground/50">
                ({filteredSessions.length}{deferredSearch ? `/${opencodeSessions.length}` : ""})
              </span>
            </div>
            {/* Search input */}
            <div className="relative mb-2">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground pointer-events-none" />
              <input
                type="text"
                placeholder={locale === "de" ? "Suchen..." : "Search..."}
                value={sessionSearch}
                onChange={(e) => setSessionSearch(e.target.value)}
                className="w-full pl-7 pr-2 py-1.5 text-xs bg-sidebar-accent/50 border border-sidebar-border rounded placeholder:text-muted-foreground/50 focus:outline-none focus:border-primary/50"
              />
              {sessionSearch && (
                <button
                  onClick={() => setSessionSearch("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
            </div>
            <div className="flex-1 overflow-y-auto space-y-1 pr-1 -mr-1">
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
                const formattedDate = session.time?.updated 
                  ? formatSessionDate(session.time.updated)
                  : null;
                return (
                  <div key={session.id}>
                    <ContextMenu>
                      <ContextMenuTrigger asChild>
                        <button
                          onClick={() => handleSessionClick(session.id)}
                          className={cn(
                            "w-full px-3 py-2 text-left transition-colors flex items-start gap-2",
                            isSelected
                              ? "bg-primary/15 border border-primary text-foreground"
                              : "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
                          )}
                        >
                          {hasChildren && (
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                toggleSessionExpanded(session.id);
                              }}
                              className="mt-0.5 p-0.5 hover:bg-muted rounded flex-shrink-0"
                            >
                              {isExpanded ? (
                                <ChevronDown className="w-3 h-3" />
                              ) : (
                                <ChevronRight className="w-3 h-3" />
                              )}
                            </button>
                          )}
                          <div className={cn("flex-1 min-w-0", !hasChildren && "ml-5")}>
                            <div className="flex items-center gap-1">
                              <span className="text-sm truncate font-medium">
                                {session.title || "Untitled"}
                              </span>
                              {hasChildren && (
                                <span className="text-[10px] text-primary/70">
                                  ({children.length})
                                </span>
                              )}
                            </div>
                            {formattedDate && (
                              <div className="text-[10px] text-foreground/50 dark:text-muted-foreground mt-0.5">
                                {formattedDate}
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
                          {locale === "de" ? "Anpinnen" : "Pin"}
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
                          const childFormattedDate = child.time?.updated
                            ? formatSessionDate(child.time.updated)
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

        {/* Collapsed session indicator */}
        {sidebarCollapsed && opencodeSessions.length > 0 && (
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
          className={`w-full ${sidebarCollapsed ? "px-2 pb-4" : "px-4 pb-6"} mt-auto pt-4`}
        >
          <div className="h-px w-full bg-primary/50 mb-3" />
          {sidebarCollapsed ? (
            // Collapsed: stack vertically
            <div className="space-y-3">
              <Button
                variant="ghost"
                size="default"
                onClick={toggleLocale}
                aria-label="Sprache wechseln"
                className="w-full px-4 py-3 text-sm font-medium flex items-center justify-center transition-colors"
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
                <Globe2 className="w-4 h-4 shrink-0" />
              </Button>
              <Button
                variant="ghost"
                size="default"
                onClick={toggleTheme}
                aria-pressed={resolvedTheme === "dark"}
                className="w-full px-4 py-3 text-sm font-medium flex items-center justify-center transition-colors"
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
                {resolvedTheme === "dark" ? (
                  <SunMedium className="w-4 h-4 shrink-0" />
                ) : (
                  <MoonStar className="w-4 h-4 shrink-0" />
                )}
              </Button>
              <Button
                variant="ghost"
                size="default"
                onClick={toggleJakSnow}
                className="w-full px-4 py-3 text-sm font-medium flex items-center justify-center transition-colors"
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
                <Snowflake className="w-4 h-4 shrink-0" />
                <span className="sr-only">JAK</span>
              </Button>
            </div>
          ) : (
            // Expanded: show in a row with just icons
            <div className="space-y-2">
              <div className="flex items-center justify-center gap-2">
                <Button
                  variant="ghost"
                  size="default"
                  onClick={toggleLocale}
                  aria-label="Sprache wechseln"
                  className="px-3 py-2 text-sm font-medium flex items-center justify-center transition-colors"
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
                  aria-pressed={resolvedTheme === "dark"}
                  className="px-3 py-2 text-sm font-medium flex items-center justify-center transition-colors"
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
                  {resolvedTheme === "dark" ? (
                    <SunMedium className="w-4 h-4" />
                  ) : (
                    <MoonStar className="w-4 h-4" />
                  )}
                </Button>
                <ContextMenu>
                  <ContextMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="default"
                      onClick={toggleJakSnow}
                      className="px-3 py-2 text-sm font-medium flex items-center justify-center transition-colors"
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
                      <Snowflake className="w-4 h-4" />
                    </Button>
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <div className="px-2 py-1.5">
                      <div className="text-xs text-muted-foreground mb-2">Volume</div>
                      <input
                        type="range"
                        min={0}
                        max={1}
                        step={0.05}
                        value={jakVolume}
                        onChange={(e) => {
                          const vol = Number(e.target.value);
                          if (Number.isNaN(vol)) return;
                          setJakVolume(vol);
                          localStorage.setItem("jakVolume", String(vol));
                          if (jakAudioRef.current) {
                            jakAudioRef.current.volume = vol;
                          }
                        }}
                        className="w-[120px]"
                        style={{ accentColor: "var(--primary)" }}
                        aria-label="JAK Volume"
                      />
                    </div>
                  </ContextMenuContent>
                </ContextMenu>
              </div>
            </div>
          )}
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

      {/* Snow overlay */}
      {showJakSnow && <SnowOverlay intensity={jakVolume} />}

      {/* Hidden audio element for JAK snow feature - optional, fails silently if file not present */}
      <audio
        ref={jakAudioRef}
        src="/audio/jingle-bells.mp3"
        loop
        preload="none"
        className="hidden"
      />

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
