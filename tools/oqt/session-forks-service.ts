// Retired-stack service registration for the pinned runtime only (ADR-0071):
// navigation logic in session-forks.ts is transport-neutral; host2 exposes it
// through its Session contracts. Delete with the pinned transport.
import { defineService } from "@earendil-works/chord";
import type { SessionForks as SessionForksApi } from "./session-forks.ts";

export type SessionForks = SessionForksApi;
export const SessionForks = defineService<SessionForksApi>("oqt.session-forks");
