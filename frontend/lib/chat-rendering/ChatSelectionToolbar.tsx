"use client";

import { cn } from "@/lib/utils";
import { Copy, Quote } from "lucide-react";
import {
	type MouseEvent as ReactMouseEvent,
	type PointerEvent as ReactPointerEvent,
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";

/**
 * Actions for a fragment of a message.
 *
 * It appears because something is selected, not because a pointer passed over
 * something. That distinction is the point: a hover-revealed row cannot be
 * reached by touch or by a keyboard, and it makes the transcript twitch as the
 * pointer crosses it. A selection is an explicit statement that this passage
 * is the one you mean.
 *
 * Two details decide whether it feels calm or twitchy:
 *
 *   1. It waits for the selection to SETTLE. Placing it on every
 *      selectionchange puts it on screen the instant a drag begins and then
 *      has it chase the cursor across the paragraph.
 *   2. It anchors to the FOCUS point — where the drag ended — rather than to
 *      the last rectangle of the range, which jumps from line to line as the
 *      selection grows.
 */

/** Distance from the selection, and from the viewport edge. */
const GAP_PX = 8;

interface ChatSelectionToolbarProps {
	/** Quote the passage into the composer. Omitted where there is no composer. */
	onQuote?: (text: string) => void;
	className?: string;
}

interface Placement {
	left: number;
	top: number;
}

/** The selected passage, but only when it lies inside a message. */
function selectionInMessage(): Range | null {
	const selection = window.getSelection();
	if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
		return null;
	}
	const range = selection.getRangeAt(0);
	if (range.toString().trim().length === 0) return null;
	const container = range.commonAncestorContainer;
	const element =
		container.nodeType === Node.ELEMENT_NODE
			? (container as Element)
			: container.parentElement;
	if (!element?.closest("[data-message-id]")) return null;
	return range;
}

/** Where the selection ended, as a rectangle to hang the toolbar from. */
function anchorRect(range: Range): DOMRect {
	const selection = window.getSelection();
	if (selection?.focusNode) {
		const caret = document.createRange();
		try {
			caret.setStart(selection.focusNode, selection.focusOffset);
			caret.collapse(true);
			const rect = caret.getBoundingClientRect();
			if (rect.height > 0) return rect;
		} catch {
			// A focus node outside the range's document: fall through.
		}
	}
	const rects = range.getClientRects();
	return rects.item(rects.length - 1) ?? range.getBoundingClientRect();
}

export function ChatSelectionToolbar({
	onQuote,
	className,
}: ChatSelectionToolbarProps) {
	const { t } = useTranslation();
	const toolbar = useRef<HTMLDivElement>(null);
	const [passage, setPassage] = useState<string | null>(null);
	const [placement, setPlacement] = useState<Placement | null>(null);

	const dismiss = useCallback(() => {
		setPassage(null);
		setPlacement(null);
	}, []);

	const settle = useCallback(() => {
		const range = selectionInMessage();
		if (!range) {
			dismiss();
			return;
		}
		setPassage(range.toString().trim());
	}, [dismiss]);

	// Position after the toolbar exists, so its width is a real measurement
	// rather than a guess that has to be corrected on a second pass.
	useLayoutEffect(() => {
		if (passage === null) return;
		const node = toolbar.current;
		const range = selectionInMessage();
		if (!node || !range) return;
		const anchor = anchorRect(range);
		const own = node.getBoundingClientRect();
		const above = anchor.top - own.height - GAP_PX;
		setPlacement({
			left: Math.min(
				Math.max(GAP_PX, anchor.left - own.width / 2),
				Math.max(GAP_PX, window.innerWidth - own.width - GAP_PX),
			),
			top: above > GAP_PX ? above : anchor.bottom + GAP_PX,
		});
	}, [passage]);

	useEffect(() => {
		// While the pointer is down the selection is still being made, so the
		// toolbar stays out of the way entirely.
		const onPointerDown = () => dismiss();
		const onPointerUp = () => window.setTimeout(settle, 0);
		const onKeyUp = (event: KeyboardEvent) => {
			if (event.shiftKey || event.key.startsWith("Arrow")) settle();
		};
		// selectionchange only ever takes it away. It never puts it up.
		const onSelectionChange = () => {
			if (!selectionInMessage()) dismiss();
		};

		document.addEventListener("pointerdown", onPointerDown);
		document.addEventListener("pointerup", onPointerUp);
		document.addEventListener("keyup", onKeyUp);
		document.addEventListener("selectionchange", onSelectionChange);
		window.addEventListener("scroll", dismiss, true);
		return () => {
			document.removeEventListener("pointerdown", onPointerDown);
			document.removeEventListener("pointerup", onPointerUp);
			document.removeEventListener("keyup", onKeyUp);
			document.removeEventListener("selectionchange", onSelectionChange);
			window.removeEventListener("scroll", dismiss, true);
		};
	}, [dismiss, settle]);

	if (passage === null) return null;

	// The pointerdown that would dismiss the toolbar, and the mousedown that
	// would collapse the selection it acts on, both have to be refused.
	const hold = (event: ReactPointerEvent | ReactMouseEvent) => {
		event.stopPropagation();
		event.preventDefault();
	};

	const finish = (act: () => void) => () => {
		act();
		window.getSelection()?.removeAllRanges();
		dismiss();
	};

	return (
		<div
			ref={toolbar}
			className={cn("chat-selection", className)}
			style={{
				left: placement?.left ?? 0,
				top: placement?.top ?? 0,
				visibility: placement ? "visible" : "hidden",
			}}
			role="toolbar"
			aria-label={t("chat.selectionActions")}
		>
			{onQuote ? (
				<button
					type="button"
					className="chat-action"
					onPointerDown={hold}
					onMouseDown={hold}
					onClick={finish(() => onQuote(passage))}
				>
					<Quote aria-hidden="true" />
					<span>{t("chat.quote")}</span>
				</button>
			) : null}
			<button
				type="button"
				className="chat-action"
				onPointerDown={hold}
				onMouseDown={hold}
				onClick={finish(() => {
					void navigator.clipboard?.writeText(passage);
				})}
			>
				<Copy aria-hidden="true" />
				<span>{t("chat.copy")}</span>
			</button>
		</div>
	);
}
