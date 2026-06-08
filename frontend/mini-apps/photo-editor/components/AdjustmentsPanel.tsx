import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { RotateCcwIcon } from "lucide-react";
import type { Adjustments } from "../types";

interface AdjustmentRowProps {
	label: string;
	value: number;
	min: number;
	max: number;
	onChange: (value: number) => void;
}

function AdjustmentRow({
	label,
	value,
	min,
	max,
	onChange,
}: AdjustmentRowProps) {
	return (
		<div className="flex flex-col gap-1.5">
			<div className="flex items-center justify-between">
				<Label className="text-xs text-muted-foreground">{label}</Label>
				<span className="text-xs tabular-nums text-muted-foreground">
					{value.toFixed(2)}
				</span>
			</div>
			<Slider
				value={[value]}
				min={min}
				max={max}
				step={0.01}
				onValueChange={(values) => onChange(values[0] ?? value)}
			/>
		</div>
	);
}

export interface AdjustmentsPanelProps {
	adjustments: Adjustments;
	disabled?: boolean;
	onChange: (key: keyof Adjustments, value: number) => void;
	onReset: () => void;
}

export function AdjustmentsPanel({
	adjustments,
	disabled,
	onChange,
	onReset,
}: AdjustmentsPanelProps) {
	return (
		<section className="flex flex-col gap-4">
			<div className="flex items-center justify-between">
				<h3 className="text-xs font-bold uppercase tracking-wide text-muted-foreground">
					Adjustments
				</h3>
				<Button
					variant="ghost"
					size="sm"
					onClick={onReset}
					disabled={disabled}
					title="Reset adjustments"
				>
					<RotateCcwIcon className="size-3.5" />
					Reset
				</Button>
			</div>
			<div
				className="flex flex-col gap-4 data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50"
				data-disabled={disabled}
			>
				<AdjustmentRow
					label="Brightness"
					value={adjustments.brightness}
					min={0}
					max={2}
					onChange={(v) => onChange("brightness", v)}
				/>
				<AdjustmentRow
					label="Contrast"
					value={adjustments.contrast}
					min={-1}
					max={1}
					onChange={(v) => onChange("contrast", v)}
				/>
				<AdjustmentRow
					label="Saturation"
					value={adjustments.saturation}
					min={-1}
					max={1}
					onChange={(v) => onChange("saturation", v)}
				/>
			</div>
		</section>
	);
}
