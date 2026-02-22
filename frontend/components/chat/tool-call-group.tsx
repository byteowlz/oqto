"use client";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { ChevronLeft, ChevronRight } from "lucide-react";
import {
	type ReactNode,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

export type ToolCallGroupItem = {
	id: string;
	label: string;
	icon: ReactNode;
	render: () => ReactNode;
};

export function ToolCallGroup({
	items,
	mode = "tabs",
	disableInteraction = false,
	className,
}: {
	items: ToolCallGroupItem[];
	mode?: "tabs" | "bar";
	disableInteraction?: boolean;
	className?: string;
}) {
	const [activeIndex, setActiveIndex] = useState(0);
	const [isOpen, setIsOpen] = useState(false);
	const scrollRef = useRef<HTMLDivElement | null>(null);
	const [canScrollLeft, setCanScrollLeft] = useState(false);
	const [canScrollRight, setCanScrollRight] = useState(false);

	const activeItem = items[activeIndex];

	const updateScrollState = useCallback(() => {
		const el = scrollRef.current;
		if (!el) return;
		const { scrollLeft, scrollWidth, clientWidth } = el;
		setCanScrollLeft(scrollLeft > 0);
		setCanScrollRight(scrollLeft + clientWidth < scrollWidth - 1);
	}, []);

	const itemCount = items.length;
	useEffect(() => {
		// itemCount triggers re-evaluation when items change
		void itemCount;
		updateScrollState();
		const el = scrollRef.current;
		if (!el) return () => {};
		const handleScroll = () => updateScrollState();
		el.addEventListener("scroll", handleScroll, { passive: true });
		const resizeObserver = new ResizeObserver(updateScrollState);
		resizeObserver.observe(el);
		return () => {
			el.removeEventListener("scroll", handleScroll);
			resizeObserver.disconnect();
		};
	}, [itemCount, updateScrollState]);

	const handleIconClick = useCallback(
		(index: number) => {
			if (mode === "bar" || disableInteraction) return;
			if (index === activeIndex && isOpen) {
				setIsOpen(false);
				return;
			}
			setActiveIndex(index);
			setIsOpen(true);
		},
		[activeIndex, disableInteraction, isOpen, mode],
	);

	const scrollBy = useCallback((delta: number) => {
		scrollRef.current?.scrollBy({ left: delta, behavior: "smooth" });
	}, []);

	const iconButtons = useMemo(
		() =>
			items.map((item, index) => (
				<button
					key={item.id}
					type="button"
					onClick={() => handleIconClick(index)}
					aria-pressed={isOpen && index === activeIndex}
					title={item.label}
					className={cn(
						"p-1 rounded-md transition-colors border",
						disableInteraction && "pointer-events-none",
						isOpen && index === activeIndex && !disableInteraction
							? "border-primary/50 bg-primary/10"
							: "border-transparent",
						!disableInteraction && "hover:bg-muted/50",
					)}
				>
					{item.icon}
				</button>
			)),
		[activeIndex, disableInteraction, handleIconClick, isOpen, items],
	);

	if (items.length === 0) {
		return null;
	}

	return (
		<div
			className={cn(
				"rounded-lg border border-border bg-card",
				mode === "bar" && "px-2 py-1",
				className,
			)}
		>
			<div
				className={cn("flex items-center gap-1", mode !== "bar" && "px-2 py-2")}
			>
				{(canScrollLeft || canScrollRight) && (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						onClick={() => scrollBy(-120)}
						disabled={!canScrollLeft || disableInteraction}
						className={cn("h-5 w-5", mode === "bar" && "h-4 w-4")}
					>
						<ChevronLeft className="h-3 w-3" />
					</Button>
				)}
				<div className="flex-1 overflow-hidden">
					<div
						ref={scrollRef}
						className="flex items-center gap-1 overflow-x-auto scrollbar-none [scrollbar-width:none] [-ms-overflow-style:none] [&::-webkit-scrollbar]:hidden"
					>
						{iconButtons}
					</div>
				</div>
				{(canScrollLeft || canScrollRight) && (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						onClick={() => scrollBy(120)}
						disabled={!canScrollRight || disableInteraction}
						className={cn("h-5 w-5", mode === "bar" && "h-4 w-4")}
					>
						<ChevronRight className="h-3 w-3" />
					</Button>
				)}
			</div>
			{mode !== "bar" && !disableInteraction && isOpen && activeItem && (
				<div className="border-t border-border px-3 py-2">
					{activeItem.render()}
				</div>
			)}
		</div>
	);
}
