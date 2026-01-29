import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { GripVertical } from "lucide-react";
import { memo, useCallback, useState } from "react";
import type {
	BuiltinCardDefinition,
	DashboardCardSpan,
	DashboardLayoutConfig,
	DashboardRegistryCard,
} from "../types";

const GRID_ROW_HEIGHT_REM = 14;

const CARD_SPAN_OPTIONS: { value: DashboardCardSpan; label: string }[] = [
	{ value: 3, label: "1x" },
	{ value: 6, label: "2x" },
	{ value: 9, label: "3x" },
	{ value: 12, label: "Full" },
];

function spanToClass(span: DashboardCardSpan): string {
	switch (span) {
		case 3:
			return "lg:col-span-3";
		case 6:
			return "lg:col-span-6";
		case 9:
			return "lg:col-span-9";
		case 12:
			return "lg:col-span-12";
		default:
			return "lg:col-span-6";
	}
}

export type DashboardGridProps = {
	layoutConfig: DashboardLayoutConfig | null;
	layoutLoading: boolean;
	layoutEditMode: boolean;
	visibleCards: Array<BuiltinCardDefinition | DashboardRegistryCard>;
	renderBuiltinCard: (id: string) => React.ReactNode;
	renderCustomCard: (card: DashboardRegistryCard) => React.ReactNode;
	onSpanChange: (id: string, span: DashboardCardSpan) => void;
	onReorder: (fromId: string, toId: string) => void;
};

export const DashboardGrid = memo(function DashboardGrid({
	layoutConfig,
	layoutLoading,
	layoutEditMode,
	visibleCards,
	renderBuiltinCard,
	renderCustomCard,
	onSpanChange,
	onReorder,
}: DashboardGridProps) {
	const [draggedCardId, setDraggedCardId] = useState<string | null>(null);

	const handleDragStart = useCallback((id: string) => {
		setDraggedCardId(id);
	}, []);

	const handleDrop = useCallback(
		(targetId: string) => {
			if (!draggedCardId) return;
			if (draggedCardId === targetId) return;
			onReorder(draggedCardId, targetId);
			setDraggedCardId(null);
		},
		[draggedCardId, onReorder],
	);

	const handleDragEnd = useCallback(() => {
		setDraggedCardId(null);
	}, []);

	if (layoutLoading || !layoutConfig) {
		return (
			<div className="text-sm text-muted-foreground">Loading dashboard...</div>
		);
	}

	return (
		<div
			className="grid grid-cols-12 gap-4 auto-rows-fr overflow-y-auto pr-1"
			style={{ gridAutoRows: `${GRID_ROW_HEIGHT_REM}rem` }}
		>
			{visibleCards.map((card) => {
				const config = layoutConfig.cards[card.id];
				const span = config?.span ?? 6;
				return (
					<div
						key={card.id}
						draggable={layoutEditMode}
						onDragStart={() => handleDragStart(card.id)}
						onDragOver={(event) => {
							if (!layoutEditMode) return;
							event.preventDefault();
						}}
						onDrop={() => {
							if (!layoutEditMode) return;
							handleDrop(card.id);
						}}
						onDragEnd={handleDragEnd}
						className={cn("col-span-12", spanToClass(span))}
					>
						<div
							className={cn(
								"relative h-full",
								layoutEditMode && "ring-1 ring-primary/40 rounded-lg",
							)}
						>
							{layoutEditMode && (
								<div className="absolute top-2 right-2 z-10 flex items-center gap-1">
									<Button variant="secondary" size="icon" className="h-7 w-7">
										<GripVertical className="h-4 w-4" />
									</Button>
									{CARD_SPAN_OPTIONS.map((option) => (
										<Button
											key={option.value}
											variant={span === option.value ? "default" : "ghost"}
											size="icon"
											className="h-7 w-7 text-[10px]"
											onClick={() => onSpanChange(card.id, option.value)}
										>
											{option.label}
										</Button>
									))}
								</div>
							)}
							{"defaultSpan" in card
								? renderBuiltinCard(card.id)
								: renderCustomCard(card)}
						</div>
					</div>
				);
			})}
		</div>
	);
});
