/**
 * A2UI DateTimeInput Component
 *
 * Date and/or time picker input.
 */

import { resolveBoundValue } from "@/lib/a2ui/types";
import { useMemo } from "react";

interface A2UIDateTimeInputProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	onDataChange?: (path: string, value: unknown) => void;
}

export function A2UIDateTimeInput({
	props,
	dataModel,
	onDataChange,
}: A2UIDateTimeInputProps) {
	const valueBound = props.value as
		| { literalString?: string; path?: string }
		| undefined;
	const enableDate = (props.enableDate as boolean) ?? true;
	const enableTime = (props.enableTime as boolean) ?? false;

	const value = resolveBoundValue(valueBound, dataModel, "");
	const path = valueBound?.path;

	// Determine input type based on enabled options
	const inputType = useMemo(() => {
		if (enableDate && enableTime) return "datetime-local";
		if (enableTime) return "time";
		return "date";
	}, [enableDate, enableTime]);

	const handleChange = (newValue: string) => {
		if (path && onDataChange) {
			onDataChange(path, newValue);
		}
	};

	return (
		<input
			type={inputType}
			value={value}
			onChange={(e) => handleChange(e.target.value)}
			className="w-full px-3 py-2 border border-input rounded-md bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
		/>
	);
}
