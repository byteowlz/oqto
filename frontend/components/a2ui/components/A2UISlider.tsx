/**
 * A2UI Slider Component
 *
 * Clean slider with current value display.
 */

import { Slider } from "@/components/ui/slider";
import type { SliderComponent } from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { useEffect, useState } from "react";

interface A2UISliderProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	onDataChange?: (path: string, value: unknown) => void;
}

export function A2UISlider({
	props,
	dataModel,
	onDataChange,
}: A2UISliderProps) {
	const sliderProps = props as unknown as SliderComponent;
	const minValue = sliderProps.minValue ?? 0;
	const maxValue = sliderProps.maxValue ?? 100;
	const initialValue = resolveBoundValue(
		sliderProps.value,
		dataModel,
		minValue,
	);

	const [value, setValue] = useState(initialValue);

	// Update local value when data model changes
	useEffect(() => {
		const newValue = resolveBoundValue(sliderProps.value, dataModel, minValue);
		setValue(newValue);
	}, [sliderProps.value, dataModel, minValue]);

	const handleChange = (newValue: number[]) => {
		const val = newValue[0];
		setValue(val);

		// Update data model if bound to a path
		if (sliderProps.value?.path && onDataChange) {
			onDataChange(sliderProps.value.path, val);
		}
	};

	// Calculate percentage for display
	const percentage = ((value - minValue) / (maxValue - minValue)) * 100;

	return (
		<div className="w-full space-y-3">
			<div className="flex items-center justify-between">
				<span className="text-xs text-muted-foreground">{minValue}</span>
				<span className="text-sm font-medium tabular-nums bg-primary/10 text-primary px-2 py-0.5 rounded">
					{value}
				</span>
				<span className="text-xs text-muted-foreground">{maxValue}</span>
			</div>
			<Slider
				value={[value]}
				onValueChange={handleChange}
				min={minValue}
				max={maxValue}
				step={1}
				className="w-full"
			/>
		</div>
	);
}
