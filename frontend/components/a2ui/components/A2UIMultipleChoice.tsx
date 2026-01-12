/**
 * A2UI MultipleChoice Component
 *
 * Single selection uses styled radio buttons.
 * Multiple selection uses toggle switches.
 */

import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import type { MultipleChoiceComponent } from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";
import { Check } from "lucide-react";
import { useEffect, useState } from "react";

interface A2UIMultipleChoiceProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	onDataChange?: (path: string, value: unknown) => void;
}

export function A2UIMultipleChoice({
	props,
	dataModel,
	onDataChange,
}: A2UIMultipleChoiceProps) {
	const choiceProps = props as unknown as MultipleChoiceComponent;
	const options = choiceProps.options || [];
	const maxSelections = choiceProps.maxAllowedSelections;
	const initialSelections = resolveBoundValue<string[]>(
		choiceProps.selections,
		dataModel,
		[],
	);

	const [selections, setSelections] = useState<string[]>(initialSelections);

	// Update local value when data model changes
	useEffect(() => {
		const newSelections = resolveBoundValue<string[]>(
			choiceProps.selections,
			dataModel,
			[],
		);
		setSelections(newSelections);
	}, [choiceProps.selections, dataModel]);

	const handleChange = (newSelections: string[]) => {
		setSelections(newSelections);

		// Update data model if bound to a path
		if (choiceProps.selections?.path && onDataChange) {
			onDataChange(choiceProps.selections.path, newSelections);
		}
	};

	// Single selection mode - styled selection cards
	if (maxSelections === 1) {
		return (
			<div className="space-y-1.5">
				{options.map((option) => {
					const label = resolveBoundValue(
						option.label,
						dataModel,
						option.value,
					);
					const isSelected = selections[0] === option.value;
					return (
						<button
							type="button"
							key={option.value}
							onClick={() => handleChange([option.value])}
							className={cn(
								"w-full flex items-center gap-3 px-3 py-2 border text-left transition-all",
								isSelected
									? "border-primary bg-primary/10 text-primary"
									: "border-border hover:border-primary/50 hover:bg-muted/50",
							)}
						>
							<div
								className={cn(
									"w-4 h-4 border-2 flex items-center justify-center transition-all",
									isSelected
										? "border-primary bg-primary"
										: "border-muted-foreground",
								)}
							>
								{isSelected && (
									<Check className="w-2.5 h-2.5 text-primary-foreground" />
								)}
							</div>
							<span className="text-sm">{label}</span>
						</button>
					);
				})}
			</div>
		);
	}

	// Multiple selection mode - toggle switches
	const toggleSelection = (value: string) => {
		let newSelections: string[];
		if (selections.includes(value)) {
			newSelections = selections.filter((v) => v !== value);
		} else {
			// Check max selections
			if (maxSelections && selections.length >= maxSelections) {
				return; // Don't add more
			}
			newSelections = [...selections, value];
		}
		handleChange(newSelections);
	};

	return (
		<div className="space-y-1">
			{options.map((option) => {
				const label = resolveBoundValue(option.label, dataModel, option.value);
				const isChecked = selections.includes(option.value);
				return (
					<div
						key={option.value}
						className="flex items-center justify-between gap-3 py-1.5"
					>
						<Label className="cursor-pointer text-sm font-normal flex-1">
							{label}
						</Label>
						<Switch
							checked={isChecked}
							onCheckedChange={() => toggleSelection(option.value)}
						/>
					</div>
				);
			})}
		</div>
	);
}
