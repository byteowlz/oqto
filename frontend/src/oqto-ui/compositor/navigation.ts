/**
 * Second explicit public seam of the compositor: directional navigation
 * queries. Kept separate from index.ts so the primary seam stays within its
 * operation budget; both files are the only legal entries to the kernel.
 */

export { neighborContainer } from "./kernel/navigation";
export type { NavigationDirection } from "./kernel/navigation";
