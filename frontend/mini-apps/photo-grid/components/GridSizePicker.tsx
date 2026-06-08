import { Button } from "@/components/ui/button";
import { MinusIcon, PlusIcon } from "lucide-react";
import { MAX_COLS, MAX_ROWS } from "../types";

interface StepperProps {
	label: string;
	value: number;
	min: number;
	max: number;
	onChange: (value: number) => void;
}

function Stepper({ label, value, min, max, onChange }: StepperProps) {
	return (
		<div className="flex items-center gap-2">
			<span className="w-12 text-xs text-muted-foreground">{label}</span>
			<Button
				variant="outline"
				size="icon"
				disabled={value <= min}
				onClick={() => onChange(value - 1)}
				aria-label={`Decrease ${label}`}
			>
				<MinusIcon className="size-4" />
			</Button>
			<span className="w-6 text-center text-sm tabular-nums">{value}</span>
			<Button
				variant="outline"
				size="icon"
				disabled={value >= max}
				onClick={() => onChange(value + 1)}
				aria-label={`Increase ${label}`}
			>
				<PlusIcon className="size-4" />
			</Button>
		</div>
	);
}

export interface GridSizePickerProps {
	rows: number;
	cols: number;
	onChange: (rows: number, cols: number) => void;
}

export function GridSizePicker({ rows, cols, onChange }: GridSizePickerProps) {
	return (
		<div className="flex flex-col gap-2">
			<Stepper
				label="Rows"
				value={rows}
				min={1}
				max={MAX_ROWS}
				onChange={(v) => onChange(v, cols)}
			/>
			<Stepper
				label="Columns"
				value={cols}
				min={1}
				max={MAX_COLS}
				onChange={(v) => onChange(rows, v)}
			/>
		</div>
	);
}
