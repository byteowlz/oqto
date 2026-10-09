/**
 * The effective keyboard bindings, layered the way ADR-0040 layers
 * configuration: defaults, then the resolved config, then what this person
 * changed on this device. Saving writes only the device layer, so a config
 * file stays the owner of everything it declares.
 */

import { useCallback, useMemo, useState } from "react";
import { bindingsFromConfig } from "../compositor/react/config-bindings";
import type { KeyBinding } from "../compositor/react/keybindings";
import {
	type BindingOverride,
	bindingStorageKey,
	readBindingOverrides,
	writeBindingOverrides,
} from "../platform/binding-storage";
import type { OqtoUiConfig } from "../platform/contracts";
import type { LayoutDocumentStore } from "../platform/layout-storage";

interface ShellBindingsInput {
	readonly storage: LayoutDocumentStore;
	readonly platformId: string;
	readonly configured: OqtoUiConfig["bindings"];
}

export interface ShellBindings {
	readonly bindings: readonly KeyBinding[];
	readonly overrides: readonly BindingOverride[];
	save(next: readonly BindingOverride[]): void;
}

export function useShellBindings(input: ShellBindingsInput): ShellBindings {
	const { storage, platformId, configured } = input;
	const [key] = useState(() => bindingStorageKey(platformId));
	const [overrides, setOverrides] = useState<readonly BindingOverride[]>(() =>
		readBindingOverrides(storage, key),
	);
	const bindings = useMemo(
		() => bindingsFromConfig(overrides, bindingsFromConfig(configured)),
		[overrides, configured],
	);
	const save = useCallback(
		(next: readonly BindingOverride[]) => {
			writeBindingOverrides(storage, key, next);
			setOverrides(next);
		},
		[storage, key],
	);
	return { bindings, overrides, save };
}
