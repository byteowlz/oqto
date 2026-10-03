import type { SelectionController } from "./selection-host";
export type SelectionHost = { session(id: string): SelectionController };
