import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { ListTodo, Sparkles, Trash2 } from "lucide-react";
import { memo, useCallback, useState } from "react";
import type {
	BuiltinCardDefinition,
	DashboardLayoutConfig,
	DashboardRegistryCard,
} from "../types";

type SidebarSection = "cards" | "custom";

function CollapsedSidebarButton({
	active,
	label,
	icon: Icon,
	onClick,
}: {
	active: boolean;
	label: string;
	icon: React.ElementType;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"w-8 h-8 flex items-center justify-center relative transition-colors rounded",
				active
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			aria-label={label}
			title={label}
		>
			<Icon className="h-4 w-4" />
		</button>
	);
}

export type DashboardSidebarProps = {
	layoutLabel: string;
	noticeLabel: string;
	customCardsLabel: string;
	collapsed: boolean;
	setCollapsed: (value: boolean | ((prev: boolean) => boolean)) => void;
	sidebarSection: SidebarSection;
	setSidebarSection: (section: SidebarSection) => void;
	layoutConfig: DashboardLayoutConfig | null;
	orderedCards: Array<BuiltinCardDefinition | DashboardRegistryCard>;
	onToggleCard: (id: string) => void;
	onRemoveCustomCard: (id: string) => void;
	onAddCustomCard: (card: {
		title: string;
		description?: string;
		type: "markdown" | "query";
		content?: string;
		url?: string;
		method?: string;
	}) => void;
};

export const DashboardSidebar = memo(function DashboardSidebar({
	layoutLabel,
	noticeLabel,
	customCardsLabel,
	collapsed,
	setCollapsed,
	sidebarSection,
	setSidebarSection,
	layoutConfig,
	orderedCards,
	onToggleCard,
	onRemoveCustomCard,
	onAddCustomCard,
}: DashboardSidebarProps) {
	const [customTitle, setCustomTitle] = useState("");
	const [customDescription, setCustomDescription] = useState("");
	const [customType, setCustomType] = useState<"markdown" | "query">(
		"markdown",
	);
	const [customContent, setCustomContent] = useState("");
	const [customUrl, setCustomUrl] = useState("");
	const [customMethod, setCustomMethod] = useState("GET");

	const handleAddCustomCard = useCallback(() => {
		const title = customTitle.trim();
		if (!title) return;
		onAddCustomCard({
			title,
			description: customDescription.trim() || undefined,
			type: customType,
			content: customType === "markdown" ? customContent : undefined,
			url: customType === "query" ? customUrl : undefined,
			method: customType === "query" ? customMethod : undefined,
		});
		setCustomTitle("");
		setCustomDescription("");
		setCustomContent("");
		setCustomUrl("");
	}, [
		customContent,
		customDescription,
		customMethod,
		customTitle,
		customType,
		customUrl,
		onAddCustomCard,
	]);

	const renderCardsManager = () => {
		if (!layoutConfig) {
			return (
				<p className="text-xs text-muted-foreground">Loading dashboard...</p>
			);
		}

		if (orderedCards.length === 0) {
			return (
				<p className="text-xs text-muted-foreground">No cards available.</p>
			);
		}

		return (
			<div className="space-y-2">
				{orderedCards.map((card) => {
					const config = layoutConfig.cards[card.id];
					const visible = config?.visible !== false;
					const isCustom = !("defaultSpan" in card);
					return (
						<div
							key={card.id}
							className="flex flex-col gap-2 rounded-md border border-border bg-muted/30 px-3 py-2"
						>
							<div className="flex items-center justify-between gap-2">
								<div className="flex items-center gap-2 min-w-0">
									<span className="text-sm font-medium truncate">
										{card.title}
									</span>
								</div>
								<div className="flex items-center gap-2">
									<Button
										variant="ghost"
										size="icon"
										onClick={() => onToggleCard(card.id)}
									>
										{visible ? "Hide" : "Show"}
									</Button>
									{isCustom && (
										<Button
											variant="ghost"
											size="icon"
											onClick={() => onRemoveCustomCard(card.id)}
										>
											<Trash2 className="h-4 w-4" />
										</Button>
									)}
								</div>
							</div>
						</div>
					);
				})}
			</div>
		);
	};

	const renderCustomManager = () => (
		<div className="space-y-3">
			<Input
				placeholder="Card title"
				value={customTitle}
				onChange={(event) => setCustomTitle(event.target.value)}
			/>
			<Input
				placeholder="Description (optional)"
				value={customDescription}
				onChange={(event) => setCustomDescription(event.target.value)}
			/>
			<Select
				value={customType}
				onValueChange={(value) => setCustomType(value as "markdown" | "query")}
			>
				<SelectTrigger size="sm">
					<SelectValue placeholder="Card type" />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="markdown">Markdown</SelectItem>
					<SelectItem value="query">Query</SelectItem>
				</SelectContent>
			</Select>
			{customType === "markdown" ? (
				<Textarea
					placeholder="Markdown content"
					value={customContent}
					onChange={(event) => setCustomContent(event.target.value)}
					rows={4}
				/>
			) : (
				<div className="space-y-2">
					<Input
						placeholder="https://api.example.com/status"
						value={customUrl}
						onChange={(event) => setCustomUrl(event.target.value)}
					/>
					<Input
						placeholder="GET"
						value={customMethod}
						onChange={(event) => setCustomMethod(event.target.value)}
					/>
				</div>
			)}
			<Button onClick={handleAddCustomCard} className="w-full">
				Add card
			</Button>
		</div>
	);

	if (collapsed) {
		return (
			<div
				className={cn(
					"bg-card border border-border flex flex-col min-h-0 h-full transition-all duration-200 w-12 items-center",
				)}
			>
				<div className="flex flex-col gap-1 p-2 h-full overflow-y-auto">
					<CollapsedSidebarButton
						active={sidebarSection === "cards"}
						label="Cards"
						icon={ListTodo}
						onClick={() => {
							setSidebarSection("cards");
							setCollapsed(false);
						}}
					/>
					<CollapsedSidebarButton
						active={sidebarSection === "custom"}
						label="Custom cards"
						icon={Sparkles}
						onClick={() => {
							setSidebarSection("custom");
							setCollapsed(false);
						}}
					/>
				</div>
			</div>
		);
	}

	return (
		<div
			className={cn(
				"bg-card border border-border flex flex-col min-h-0 h-full transition-all duration-200 flex-[2] min-w-[320px] max-w-[420px]",
			)}
		>
			<div className="flex flex-col h-full min-h-0">
				<div className="px-4 py-3 border-b border-border">
					<div>
						<p className="text-sm font-semibold">{layoutLabel}</p>
						<p className="text-xs text-muted-foreground">{noticeLabel}</p>
					</div>
					<div className="mt-3 flex gap-1">
						<button
							type="button"
							onClick={() => setSidebarSection("cards")}
							className={cn(
								"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
								sidebarSection === "cards"
									? "bg-primary/15 text-foreground border border-primary"
									: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
							)}
							title="Cards"
						>
							<ListTodo className="h-4 w-4" />
						</button>
						<button
							type="button"
							onClick={() => setSidebarSection("custom")}
							className={cn(
								"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
								sidebarSection === "custom"
									? "bg-primary/15 text-foreground border border-primary"
									: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
							)}
							title={customCardsLabel}
						>
							<Sparkles className="h-4 w-4" />
						</button>
					</div>
				</div>
				<div className="flex-1 min-h-0 overflow-y-auto p-4">
					{sidebarSection === "cards"
						? renderCardsManager()
						: renderCustomManager()}
				</div>
			</div>
		</div>
	);
});
