/**
 * A2UI CheckBox Component
 *
 * Renders as a modern horizontal toggle switch instead of a checkbox.
 */

import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import type { CheckBoxComponent } from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { useEffect, useState } from "react";

interface A2UICheckBoxProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	onDataChange?: (path: string, value: unknown) => void;
}

export function A2UICheckBox({
	props,
	dataModel,
	onDataChange,
}: A2UICheckBoxProps) {
	const checkboxProps = props as unknown as CheckBoxComponent;
	const label = resolveBoundValue(checkboxProps.label, dataModel, "");
	const initialValue = resolveBoundValue(checkboxProps.value, dataModel, false);

	const [checked, setChecked] = useState(initialValue);

	// Update local value when data model changes
	useEffect(() => {
		const newValue = resolveBoundValue(checkboxProps.value, dataModel, false);
		setChecked(newValue);
	}, [checkboxProps.value, dataModel]);

	const handleChange = (newValue: boolean) => {
		setChecked(newValue);

		// Update data model if bound to a path
		if (checkboxProps.value?.path && onDataChange) {
			onDataChange(checkboxProps.value.path, newValue);
		}
	};

	return (
		<div className="flex items-center justify-between gap-3 py-1">
			{label && (
				<Label className="cursor-pointer text-sm font-normal flex-1">
					{label}
				</Label>
			)}
			<Switch checked={checked} onCheckedChange={handleChange} />
		</div>
	);
}
