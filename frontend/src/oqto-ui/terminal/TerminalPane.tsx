/**
 * A live shell in the work directory as placeable Content. The emulator is
 * wterm's DOM renderer over libghostty's VT core: text in the terminal is
 * real text, so selection, find and screen readers work on it. Everything
 * that reaches the host goes through the terminal port, so the pane itself
 * knows nothing about sockets.
 */

import "@wterm/dom/css";
import { useMountEffect } from "@/hooks/use-mount-effect";
import ghosttyWasm from "@wterm/ghostty/ghostty-vt.wasm?url";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	TerminalHost,
	TerminalSession,
} from "../platform/terminal-contract";

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

		void (async () => {
			const [{ WTerm }, { GhosttyCore }] = await Promise.all([
				import("@wterm/dom"),
				import("@wterm/ghostty"),
			]);
			// Vite serves the VT binary as an asset; the core cannot find it on
			// its own from an optimized dependency.
			const core = await GhosttyCore.load({ wasmPath: ghosttyWasm });
			if (disposed || !surface.current) return;
			const terminal = new WTerm(surface.current, {
				core,
				cursorBlink: true,
				// The Container is resizable, so the emulator follows its own box
				// and reports the new grid; the PTY follows that report.
				autoResize: true,
				onData: (data: string) => session?.input(data),
				onResize: (cols: number, rows: number) =>
					session?.resize({ cols, rows }),
			});
			dispose = () => terminal.destroy();
			await terminal.init();
			if (disposed) return;
			try {
				session = await terminalHost.open(workspacePath, START_SIZE, (bytes) =>
					terminal.write(bytes),
				);
			} catch {
				if (!disposed) setFailed(true);
				return;
			}
			if (disposed) session.close();
		})();

		return () => {
			disposed = true;
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
