/**
 * A2UI TextField Component
 */

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import type { TextFieldComponent } from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { useEffect, useState } from "react";

interface A2UITextFieldProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	onDataChange?: (path: string, value: unknown) => void;
}

export function A2UITextField({
	props,
	dataModel,
	onDataChange,
}: A2UITextFieldProps) {
	const fieldProps = props as unknown as TextFieldComponent;
	const label = resolveBoundValue(fieldProps.label, dataModel, "");
	const initialValue = resolveBoundValue(fieldProps.text, dataModel, "");
	const fieldType = fieldProps.textFieldType || "shortText";
	const validationRegexp = fieldProps.validationRegexp;

	const [value, setValue] = useState(initialValue);
	const [isValid, setIsValid] = useState(true);

	// Update local value when data model changes
	useEffect(() => {
		const newValue = resolveBoundValue(fieldProps.text, dataModel, "");
		setValue(newValue);
	}, [fieldProps.text, dataModel]);

	const handleChange = (newValue: string) => {
		setValue(newValue);

		// Validate if regexp provided
		if (validationRegexp) {
			const regex = new RegExp(validationRegexp);
			setIsValid(regex.test(newValue));
		}

		// Update data model if bound to a path
		if (fieldProps.text?.path && onDataChange) {
			onDataChange(fieldProps.text.path, newValue);
		}
	};

	const inputType =
		fieldType === "number"
			? "number"
			: fieldType === "obscured"
				? "password"
				: fieldType === "date"
					? "date"
					: "text";

	const isLongText = fieldType === "longText";

	return (
		<div className="space-y-1.5">
			{label && <Label>{label}</Label>}
			{isLongText ? (
				<Textarea
					value={value}
					onChange={(e) => handleChange(e.target.value)}
					className={!isValid ? "border-destructive" : ""}
					rows={4}
				/>
			) : (
				<Input
					type={inputType}
					value={value}
					onChange={(e) => handleChange(e.target.value)}
					className={!isValid ? "border-destructive" : ""}
				/>
			)}
		</div>
	);
}
