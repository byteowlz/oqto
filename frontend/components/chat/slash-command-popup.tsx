"use client";

import { type SlashCommand, filterCommands } from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import { Command, Eye, Inbox, Mail, Sparkles, Terminal } from "lucide-react";
import { useEffect, useRef, useState } from "react";

// Map icon names to components
const iconMap: Record<string, React.ComponentType<{ className?: string }>> = {
	Sparkles,
	Eye,
	Inbox,
	Mail,
	Terminal,
};

interface SlashCommandPopupProps {
	commands: SlashCommand[];
	query: string;
	isOpen: boolean;
	onSelect: (command: SlashCommand) => void;
	onClose: () => void;
	className?: string;
}

export function SlashCommandPopup({
	commands,
	query,
	isOpen,
	onSelect,
	onClose,
	className,
}: SlashCommandPopupProps) {
	const [selectedIndex, setSelectedIndex] = useState(0);
	const listRef = useRef<HTMLDivElement>(null);
	const filteredCommands = filterCommands(commands, query);

	// Reset selection when query changes
	useEffect(() => {
		const nextIndex = query.length > 0 ? 0 : 0;
		setSelectedIndex(nextIndex);
	}, [query]);

	// Scroll selected item into view
	useEffect(() => {
		if (!listRef.current) return;
		const selectedEl = listRef.current.querySelector(
			`[data-index="${selectedIndex}"]`,
		);
		if (selectedEl) {
			selectedEl.scrollIntoView({ block: "nearest" });
		}
	}, [selectedIndex]);

	// Handle keyboard navigation
	useEffect(() => {
		if (!isOpen) return;

		const handleKeyDown = (e: KeyboardEvent) => {
			switch (e.key) {
				case "ArrowDown":
					e.preventDefault();
					setSelectedIndex((prev) =>
						prev < filteredCommands.length - 1 ? prev + 1 : prev,
					);
					break;
				case "ArrowUp":
					e.preventDefault();
					setSelectedIndex((prev) => (prev > 0 ? prev - 1 : prev));
					break;
				case "Enter":
				case "Tab":
					e.preventDefault();
					if (filteredCommands[selectedIndex]) {
						onSelect(filteredCommands[selectedIndex]);
					}
					break;
				case "Escape":
					e.preventDefault();
					onClose();
					break;
			}
		};

		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [isOpen, filteredCommands, selectedIndex, onSelect, onClose]);

	if (!isOpen || filteredCommands.length === 0) return null;

	return (
		<div
			ref={listRef}
			className={cn(
				"absolute bottom-full left-0 mb-2 w-72 max-h-64 overflow-y-auto",
				"bg-popover border border-border rounded-lg shadow-lg",
				"z-50",
				className,
			)}
		>
			<div className="p-1">
				{filteredCommands.map((cmd, index) => {
					const Icon = cmd.icon ? iconMap[cmd.icon] : Command;
					return (
						<button
							type="button"
							key={cmd.name}
							data-index={index}
							onClick={() => onSelect(cmd)}
							onMouseEnter={() => setSelectedIndex(index)}
							className={cn(
								"w-full flex items-center gap-3 px-3 py-2 rounded-md text-left",
								"transition-colors",
								index === selectedIndex
									? "bg-accent text-accent-foreground"
									: "hover:bg-muted",
							)}
						>
							{Icon && (
								<Icon className="w-4 h-4 text-muted-foreground shrink-0" />
							)}
							<div className="flex-1 min-w-0">
								<div className="text-sm font-medium">/{cmd.name}</div>
								<div className="text-xs text-muted-foreground truncate">
									{cmd.description}
								</div>
							</div>
						</button>
					);
				})}
			</div>
		</div>
	);
}
