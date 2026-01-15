// Main Chat Plugin for OpenCode
//
// This plugin provides:
// 1. Custom compaction prompts for Main Chat sessions
// 2. Local persistence of summaries/decisions/insights as JSONL
// 3. History injection at session start

import type { Plugin } from "@opencode-ai/plugin";
import { promises as fs } from "node:fs";
import path from "node:path";

interface HistoryEntry {
  ts: string;
  type: "summary" | "decision" | "handoff" | "insight";
  content: string;
  session_id?: string;
  meta?: Record<string, unknown>;
}

const HISTORY_DIR = path.join(".opencode", "main-chat");
const HISTORY_FILE = path.join(HISTORY_DIR, "history.jsonl");
const CONFIG_FILE = path.join(HISTORY_DIR, "config.json");
const MAIN_CHAT_TITLE_PREFIX = "[[main]]";
const MAIN_CHAT_TITLE_FALLBACK = "Main chat";

interface MainChatConfig {
  history?: {
    maxEntries?: number;
  };
  context?: {
    maxTokens?: number;
    maxRatio?: number;
  };
}

const DEFAULT_CONFIG: MainChatConfig = {
  history: {
    maxEntries: 10,
  },
  context: {},
};

/**
 * Read recent history entries from the local JSONL store.
 */
async function readRecentHistory(limit: number = 10): Promise<HistoryEntry[]> {
  try {
    const raw = await fs.readFile(HISTORY_FILE, "utf8");
    const lines = raw
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    const parsed = lines
      .map((line) => {
        try {
          return JSON.parse(line) as HistoryEntry;
        } catch {
          return null;
        }
      })
      .filter((entry): entry is HistoryEntry => Boolean(entry));
    return parsed.slice(-limit);
  } catch (error: unknown) {
    if (error && typeof error === "object" && "code" in error) {
      if ((error as { code?: string }).code === "ENOENT") {
        return [];
      }
    }
    console.error("Error reading history:", error);
    return [];
  }
}

async function loadConfig(): Promise<MainChatConfig> {
  try {
    const raw = await fs.readFile(CONFIG_FILE, "utf8");
    return { ...DEFAULT_CONFIG, ...(JSON.parse(raw) as MainChatConfig) };
  } catch (error: unknown) {
    if (error && typeof error === "object" && "code" in error) {
      if ((error as { code?: string }).code === "ENOENT") {
        return DEFAULT_CONFIG;
      }
    }
    console.error("Error reading config:", error);
    return DEFAULT_CONFIG;
  }
}

function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

function resolveContextLimit(
  config: MainChatConfig,
  sessionMaxContext: number | undefined,
): number | undefined {
  const maxTokens = config.context?.maxTokens;
  const maxRatio = config.context?.maxRatio;
  let limit: number | undefined = maxTokens;

  if (maxRatio && sessionMaxContext && sessionMaxContext > 0) {
    const ratioLimit = Math.floor(sessionMaxContext * maxRatio);
    limit = typeof limit === "number" ? Math.min(limit, ratioLimit) : ratioLimit;
  }

  return limit;
}

function selectEntriesForContext(
  entries: HistoryEntry[],
  maxTokens?: number,
): HistoryEntry[] {
  if (!maxTokens) return entries;
  const headerTokens = estimateTokens("## Recent Context (from previous sessions)");
  let used = headerTokens;
  const selected: HistoryEntry[] = [];

  for (let i = entries.length - 1; i >= 0; i -= 1) {
    const entry = entries[i];
    const prefix = entry.type === "decision" ? "[decision]" :
      entry.type === "handoff" ? "[handoff]" :
      entry.type === "insight" ? "[insight]" :
      "[summary]";
    const line = `${prefix} ${entry.content}`;
    const tokens = estimateTokens(line) + 2;
    if (used + tokens > maxTokens) {
      break;
    }
    used += tokens;
    selected.push(entry);
  }

  return selected.reverse();
}

/**
 * Append history entries to the local JSONL store.
 */
async function appendHistory(entries: Omit<HistoryEntry, "ts">[]): Promise<void> {
  if (entries.length === 0) return;
  await fs.mkdir(HISTORY_DIR, { recursive: true });
  const now = new Date().toISOString();
  const lines = entries.map((entry) =>
    JSON.stringify({ ...entry, ts: now }),
  );
  await fs.appendFile(HISTORY_FILE, `${lines.join("\n")}\n`, "utf8");
}

/**
 * Format history entries for context injection.
 */
function formatHistoryForContext(entries: HistoryEntry[]): string {
  if (entries.length === 0) {
    return "";
  }

  const lines = entries.map((entry) => {
    const prefix = entry.type === "decision" ? "[decision]" :
                   entry.type === "handoff" ? "[handoff]" :
                   entry.type === "insight" ? "[insight]" :
                   "[summary]";
    return `${prefix} ${entry.content}`;
  });

  return `## Recent Context (from previous sessions)\n\n${lines.join("\n\n")}`;
}

/**
 * Parse compaction output into structured entries.
 */
function parseCompactionOutput(text: string): HistoryEntry[] {
  const entries: HistoryEntry[] = [];
  
  // Look for tagged sections
  const decisionRegex = /\[decision\]\s*(.+?)(?=\[|\n\n|$)/gi;
  const handoffRegex = /\[handoff\]\s*(.+?)(?=\[|\n\n|$)/gi;
  const insightRegex = /\[insight\]\s*(.+?)(?=\[|\n\n|$)/gi;
  
  let match;
  
  while ((match = decisionRegex.exec(text)) !== null) {
    entries.push({ ts: "", type: "decision", content: match[1].trim() });
  }
  
  while ((match = handoffRegex.exec(text)) !== null) {
    entries.push({ ts: "", type: "handoff", content: match[1].trim() });
  }
  
  while ((match = insightRegex.exec(text)) !== null) {
    entries.push({ ts: "", type: "insight", content: match[1].trim() });
  }
  
  // If no structured content found, treat the whole thing as a summary
  if (entries.length === 0 && text.trim()) {
    entries.push({ ts: "", type: "summary", content: text.trim() });
  }
  
  return entries;
}

export const MainChatPlugin: Plugin = async (ctx) => {
  console.log("Main Chat plugin loaded");

  // Track current session for event handlers
  let currentSessionId: string | undefined;
  // Track sessions where we've already injected history (to avoid duplicates)
  const injectedSessions = new Set<string>();
  const updatingTitles = new Set<string>();
  const handledSummaryMessages = new Set<string>();
  const sessionTitles = new Map<string, string | undefined>();
  const sessionContextLimits = new Map<string, number>();

  async function ensureMainTitle(sessionId: string, title?: string): Promise<void> {
    if (updatingTitles.has(sessionId)) return;
    const baseTitle = title?.trim() || MAIN_CHAT_TITLE_FALLBACK;
    if (baseTitle.startsWith(MAIN_CHAT_TITLE_PREFIX)) {
      return;
    }
    const prefixedTitle = `${MAIN_CHAT_TITLE_PREFIX} ${baseTitle}`;
    try {
      updatingTitles.add(sessionId);
      await ctx.client.session.update({
        path: { id: sessionId },
        body: { title: prefixedTitle },
      });
    } catch (error) {
      console.error("Failed to update session title:", error);
    } finally {
      updatingTitles.delete(sessionId);
    }
  }

  /**
   * Inject history context into a new session.
   * Uses noReply to silently provide context without triggering a response.
   */
  async function injectHistoryContext(sessionId: string): Promise<void> {
    // Skip if already injected for this session
    if (injectedSessions.has(sessionId)) {
      return;
    }

    try {
      const config = await loadConfig();
      const rawHistory = await readRecentHistory(config.history?.maxEntries ?? 10);
      const maxTokens = resolveContextLimit(
        config,
        sessionContextLimits.get(sessionId),
      );
      const history = selectEntriesForContext(rawHistory, maxTokens);
      if (history.length === 0) {
        console.log("No history to inject");
        return;
      }

      const contextBlock = formatHistoryForContext(history);
      
      // Send as a noReply system message - this provides context without generating a response
      // The assistant will have access to this context for the entire session
      await ctx.session.sendMessage({
        sessionId,
        content: `<system-context>
${contextBlock}

This context is from your previous sessions.
Use this to maintain continuity and reference past decisions when relevant.
</system-context>`,
        noReply: true,
      });

      injectedSessions.add(sessionId);
      console.log(`Injected history context into session ${sessionId}`);
    } catch (error) {
      console.error("Error injecting history context:", error);
    }
  }

  return {
    "chat.params": async (input, output) => {
      sessionContextLimits.set(input.sessionID, input.model.limit.context);
      return output;
    },
    // Custom compaction prompt
    "experimental.session.compacting": async (input, output) => {
      if (input.sessionID && !sessionContextLimits.has(input.sessionID)) {
        sessionContextLimits.set(input.sessionID, 0);
      }
      // Inject recent history as additional context for compaction
      const config = await loadConfig();
      const rawHistory = await readRecentHistory(
        config.history?.maxEntries ?? 10,
      );
      const maxTokens = resolveContextLimit(
        config,
        sessionContextLimits.get(input.sessionID),
      );
      const history = selectEntriesForContext(rawHistory, maxTokens);
      if (history.length > 0) {
        output.context.push(formatHistoryForContext(history));
      }

      // Custom compaction prompt for Main Chat
      output.prompt = `You are summarizing a Main Chat session for continuation.

This is a persistent assistant that maintains context across sessions. Extract and format the key information:

1. **Decisions** - Important choices or conclusions made during this session
   Format each as: [decision] <description>

2. **Handoffs** - Current state and next steps for continuity
   Format each as: [handoff] <description>

3. **Insights** - Learnings or patterns worth remembering long-term
   Format each as: [insight] <description>

Be concise but capture the essential context needed to continue effectively in the next session.
If there are no items for a category, omit it entirely.

Focus on what would be most useful for the assistant to know when resuming work later.`;
    },

    // Handle events
    event: async ({ event }) => {
      if (event.type === "session.created" || event.type === "session.updated") {
        const info = event.properties.info;
        if (info?.id) {
          sessionTitles.set(info.id, info.title ?? undefined);
          await ensureMainTitle(info.id, info.title ?? undefined);
        }
      }

      // Track session creation and inject history
      if (event.type === "session.created") {
        const info = event.properties.info;
        if (info?.id) {
          currentSessionId = info.id;

          // Inject history context for the new session
          await injectHistoryContext(info.id);
        }
      }

      // Also inject when session is selected/resumed (in case it's an existing session)
      if (event.type === "session.selected" || event.type === "session.resumed") {
        const props = event.properties as { id?: string; sessionID?: string };
        const sessionId = props.id || props.sessionID;
        if (sessionId) {
          currentSessionId = sessionId;
          await injectHistoryContext(sessionId);
        }
      }

      if (event.type === "message.updated") {
        const info = event.properties.info;
        if (!info?.summary || info.role !== "assistant" || !info.finish) {
          return;
        }
        const title = sessionTitles.get(info.sessionID);
        if (!title?.startsWith(MAIN_CHAT_TITLE_PREFIX)) {
          return;
        }
        if (handledSummaryMessages.has(info.id)) {
          return;
        }
        handledSummaryMessages.add(info.id);

        try {
          const message = await ctx.client.session.message({
            path: { id: info.sessionID, messageID: info.id },
          });
          const text = message.data.parts
            .filter((part) => part.type === "text")
            .map((part) => part.text)
            .join("\n\n")
            .trim();
          if (!text) return;
          const entries = parseCompactionOutput(text);
          await appendHistory(
            entries.map((entry) => ({
              ...entry,
              session_id: info.sessionID,
            })),
          );
          console.log(`Saved ${entries.length} history entries`);
        } catch (error) {
          console.error("Failed to persist compaction summary:", error);
        }
      }
    },
  };
};

export default MainChatPlugin;
