/**
 * A live shell in the work directory as placeable Content. The terminal
 * emulator is a browser library driven through a ref; everything it needs
 * to reach the host goes through the terminal port, so the pane itself
 * knows nothing about sockets.
 */

import { useMountEffect } from "@/hooks/use-mount-effect";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { subscribeElementSize } from "../platform/element-size";
import type {
	TerminalHost,
	TerminalSession,
	TerminalSize,
} from "../platform/terminal-contract";
import { readTerminalTheme } from "../platform/terminal-theme";

interface TerminalPaneProps {
	readonly terminalHost: TerminalHost;
	/** Host path of the work directory the shell starts in. */
	readonly workspacePath: string;
}

const START_SIZE = { cols: 80, rows: 24 };

export function TerminalPane({
	terminalHost,
	workspacePath,
}: TerminalPaneProps) {
	const { t } = useTranslation();
	const surface = useRef<HTMLDivElement | null>(null);
	const [failed, setFailed] = useState(false);

	useMountEffect(() => {
		let session: TerminalSession | null = null;
		let disposed = false;
		let dispose: (() => void) | null = null;
		let unobserve: (() => void) | null = null;

		void (async () => {
			const ghostty = await import("ghostty-web");
			// The wasm module has to be initialised before any Terminal exists.
			await ghostty.init();
			if (disposed || !surface.current) return;
			const theme = readTerminalTheme();
			const terminal = new ghostty.Terminal({
				fontFamily: theme.fontFamily,
				fontSize: 12,
				cursorBlink: true,
				convertEol: true,
				theme: { background: theme.background, foreground: theme.foreground },
			});
			const fit = new ghostty.FitAddon();
			terminal.loadAddon(fit);
			terminal.open(surface.current);
			fit.fit();
			dispose = () => terminal.dispose();
			try {
				session = await terminalHost.open(
					workspacePath,
					{
						cols: terminal.cols || START_SIZE.cols,
						rows: terminal.rows || START_SIZE.rows,
					},
					(bytes) => terminal.write(bytes),
				);
			} catch {
				if (!disposed) setFailed(true);
				return;
			}
			if (disposed) {
				session.close();
				return;
			}
			terminal.onData((data: string) => session?.input(data));
			terminal.onResize((size: TerminalSize) => session?.resize(size));
			// The Container is resizable, so the PTY follows the pane's own size.
			unobserve = subscribeElementSize(surface.current, () => fit.fit());
		})();

		return () => {
			disposed = true;
			unobserve?.();
			session?.close();
			dispose?.();
		};
	});

	return (
		<section className="wb-terminal" aria-label={t("oqtoUi.terminal.label")}>
			{failed ? (
				<p className="wb-terminal__note">{t("oqtoUi.terminal.failed")}</p>
			) : null}
			<div className="wb-terminal__surface" ref={surface} />
		</section>
	);
}
