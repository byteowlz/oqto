import { Button } from "@/components/ui/button";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import type { Base24Scheme, ThemeMode } from "@/mini-apps/theming";
import { MoonIcon, SunIcon } from "lucide-react";

export interface SchemePickerProps {
	schemes: ReadonlyArray<Base24Scheme>;
	schemeId: string;
	mode: ThemeMode;
	onSelectScheme: (id: string) => void;
	onToggleMode: () => void;
}

export function SchemePicker({
	schemes,
	schemeId,
	mode,
	onSelectScheme,
	onToggleMode,
}: SchemePickerProps) {
	return (
		<div className="flex items-center gap-2">
			<Select value={schemeId} onValueChange={onSelectScheme}>
				<SelectTrigger size="sm" className="w-32 sm:w-44">
					<SelectValue placeholder="Scheme" />
				</SelectTrigger>
				<SelectContent>
					{schemes.map((scheme) => (
						<SelectItem key={scheme.id} value={scheme.id}>
							{scheme.name}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
			<Button
				variant="outline"
				size="icon"
				onClick={onToggleMode}
				aria-label={mode === "dark" ? "Switch to light" : "Switch to dark"}
				title={mode === "dark" ? "Switch to light" : "Switch to dark"}
			>
				{mode === "dark" ? (
					<MoonIcon className="size-4" />
				) : (
					<SunIcon className="size-4" />
				)}
			</Button>
		</div>
	);
}
