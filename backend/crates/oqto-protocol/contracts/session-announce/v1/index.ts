/** Session announce v1. A hint, never authority: the Runner validates the named file. */
export const SESSION_ANNOUNCE_VERSION = 1 as const;
export const SESSION_ANNOUNCE_KIND = "session_announce" as const;
export const MAX_SESSION_ANNOUNCE_BYTES = 8 * 1024;
export const SESSION_ANNOUNCE_SOCKET_NAME = "pi-session-announce.sock";
export const SESSION_ANNOUNCE_POINTER_NAME = "oqto-session-announce.path";

export type AnnounceReason = "start" | "new" | "switch" | "fork" | "resume";

export interface SessionAnnounce {
  v: typeof SESSION_ANNOUNCE_VERSION;
  kind: typeof SESSION_ANNOUNCE_KIND;
  reason: AnnounceReason;
  /** Absolute, canonical native session file. */
  session_file: string;
  /** Native session id from that file's header, [A-Za-z0-9_-]{1,256}. */
  session_id: string;
  /** Absolute, canonical working directory. */
  cwd: string;
  pid: number;
  /** pi-tui-rpc socket hint; need not exist yet. */
  live_socket?: string | null;
  /** Fork lineage hint; never read by the Runner. */
  previous_session_file?: string | null;
}

export type AnnounceOutcome = "listed" | "duplicate" | "denied" | "invalid";

export type AnnounceInvalidCode =
  | "empty"
  | "not_utf8"
  | "malformed"
  | "wrong_kind"
  | "wrong_version"
  | "invalid_path"
  | "invalid_id"
  | "invalid_pid"
  | "too_large";

export interface SessionAnnounceAck {
  v: typeof SESSION_ANNOUNCE_VERSION;
  /** True only for `listed` and `duplicate`. */
  ok: boolean;
  result: AnnounceOutcome;
  /** Stable code for `invalid`, short reason otherwise. */
  detail: string;
}
