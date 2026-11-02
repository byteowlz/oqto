// Retired-stack service registration for the pinned runtime only (ADR-0071):
// the receipt logic in operator-receipts.ts is transport-neutral; host2 exposes
// it as a chord2 contract instead. Delete with the pinned transport.
import { defineService } from "@earendil-works/chord";
import type { OperatorResults as OperatorResultsApi } from "./operator-receipts.ts";

export const OperatorResults = defineService<OperatorResultsApi>(
	"oq.operator-results",
);
