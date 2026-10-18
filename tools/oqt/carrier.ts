import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

/** Protected owner channel. Role and binding come from composition, not frames. */
export class Carrier {
	readonly child;
	replies: Array<{
		resolve: (value: unknown) => void;
		reject: (error: Error) => void;
		timer: ReturnType<typeof setTimeout>;
	}> = [];
	constructor(
		role: "tools" | "operator",
		directory: string,
		binding: object,
		/** Coding ceiling source: the carrier loads the one sandbox config
		 * itself (ADR-0060); absent for the demo. */
		ceiling?: object,
	) {
		const binary = process.env.OQT_CARRIER;
		if (!binary)
			throw new Error("Missing protected carrier; refusing fallback");
		this.child = spawn(binary, [role, directory], {
			stdio: ["pipe", "pipe", "inherit"],
			env: {
				PATH: process.env.PATH,
				OQ_ADAPTIVE_SECCOMP_BPF: process.env.OQT_SECCOMP_BPF,
				OQTO_SANDBOX_SHIM_BIN: process.env.OQT_SANDBOX_SHIM,
				OQ_DEMO_BINDING: JSON.stringify(binding),
				...(ceiling ? { OQT_CEILING_SOURCE: JSON.stringify(ceiling) } : {}),
			},
		});
		const lines = createInterface({ input: this.child.stdout });
		lines.on("line", (line) => {
			const call = this.replies.shift();
			if (!call) return;
			clearTimeout(call.timer);
			try {
				const value: unknown = JSON.parse(line);
				if (typeof value !== "object" || value === null || !("ok" in value))
					throw new Error("Invalid carrier envelope");
				if (value.ok !== true)
					call.reject(
						new Error(
							"error" in value ? String(value.error) : "Carrier failed",
						),
					);
				else call.resolve("result" in value ? value.result : undefined);
			} catch (error) {
				call.reject(new Error(String(error)));
			}
		});
		this.child.on("error", (error) => this.rejectAll(error));
		this.child.on("exit", () => {
			lines.close();
			this.rejectAll(new Error("Permission carrier disconnected"));
		});
	}
	rejectAll(error: Error) {
		for (const call of this.replies.splice(0)) {
			clearTimeout(call.timer);
			call.reject(error);
		}
	}
	/** `deadlineMs`: per-call budget; startup calls (open + recovery) get more. */
	call(command: object, deadlineMs = 5000): Promise<unknown> {
		const stdin = this.child.stdin;
		if (
			this.child.exitCode !== null ||
			this.child.signalCode !== null ||
			stdin.destroyed ||
			stdin.writableEnded
		)
			return Promise.reject(new Error("Permission carrier is closed"));
		const frame = `${JSON.stringify(command)}\n`;
		if (Buffer.byteLength(frame) > 16384)
			return Promise.reject(new Error("Carrier frame exceeds limit"));
		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => {
				this.rejectAll(new Error("Carrier deadline"));
				this.child.kill();
			}, deadlineMs);
			this.replies.push({ resolve, reject, timer });
			stdin.write(frame, (error) => {
				if (error) {
					this.replies = this.replies.filter(
						(entry) => entry.resolve !== resolve,
					);
					clearTimeout(timer);
					reject(new Error("Permission carrier write failed"));
				}
			});
		});
	}
	async close() {
		if (this.child.exitCode !== null || this.child.signalCode !== null) return;
		this.child.stdin.end();
		await new Promise<void>((resolve) => {
			const timer = setTimeout(() => this.child.kill("SIGKILL"), 1000);
			this.child.once("exit", () => {
				clearTimeout(timer);
				resolve();
			});
		});
	}
}
