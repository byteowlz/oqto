import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { CropIcon, MoveIcon } from "lucide-react";
import type { Tool } from "../types";

interface ToolButtonProps {
	active: boolean;
	label: string;
	onClick: () => void;
	children: React.ReactNode;
}

function ToolButton({ active, label, onClick, children }: ToolButtonProps) {
	return (
		<Button
			variant={active ? "default" : "ghost"}
			size="icon"
			onClick={onClick}
			aria-pressed={active}
			title={label}
			className={cn(active && "pointer-events-none")}
		>
			{children}
		</Button>
	);
}

export interface ToolbarProps {
	tool: Tool;
	disabled?: boolean;
	onSelectTool: (tool: Tool) => void;
}

export function Toolbar({ tool, disabled, onSelectTool }: ToolbarProps) {
	return (
		<div
			className="flex flex-row gap-1 border-b border-border p-2 data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-40 md:flex-col md:border-b-0 md:border-r"
			data-disabled={disabled}
		>
			<ToolButton
				active={tool === "move"}
				label="Move"
				onClick={() => onSelectTool("move")}
			>
				<MoveIcon className="size-4" />
			</ToolButton>
			<ToolButton
				active={tool === "crop"}
				label="Crop"
				onClick={() => onSelectTool("crop")}
			>
				<CropIcon className="size-4" />
			</ToolButton>
		</div>
	);
}
