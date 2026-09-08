import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useMountEffect } from "@/hooks/use-mount-effect";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export type LoginCommand =
	| { command: "providers" }
	| { command: "start"; provider: string; method: "oauth" | "api_key" }
	| { command: "status" | "cancel"; attempt: string }
	| { command: "answer"; attempt: string; prompt: string; value: string };
export type ProviderLoginPort = {
	call: (command: LoginCommand, signal?: AbortSignal) => Promise<unknown>;
};
type Provider = {
	id: string;
	name: string;
	configured: boolean;
	methods: ("oauth" | "api_key")[];
};
type Event = {
	id: string;
	type: string;
	message?: string;
	url?: string;
	instructions?: string;
	verificationUri?: string;
	userCode?: string;
	links?: { url: string; label: string }[];
};
type Prompt = {
	id: string;
	type: "text" | "secret" | "manual_code" | "select";
	message: string;
	options?: { id: string; label: string }[];
};
type Attempt = {
	provider: string;
	id: string;
	state: string;
	events: Event[];
	prompt: Prompt | null;
};
const terminal = new Set([
	"saved",
	"saved_refresh_required",
	"failed",
	"cancelled",
	"expired",
]);
export function safeLoginUrl(value: unknown): string | undefined {
	try {
		const url = new URL(typeof value === "string" ? value : "");
		return url.protocol === "https:" && !url.username && !url.password
			? url.href
			: undefined;
	} catch {
		return undefined;
	}
}
function record(value: unknown): Record<string, unknown> {
	if (!value || typeof value !== "object" || Array.isArray(value))
		throw new Error("Invalid authentication response");
	return value as Record<string, unknown>;
}
export function parseLoginAttempt(value: unknown): Attempt {
	const row = record(value);
	if (
		typeof row.id !== "string" ||
		typeof row.provider !== "string" ||
		typeof row.state !== "string" ||
		!["running", "cancelling", "awaiting_commit", ...terminal].includes(
			row.state,
		) ||
		!Array.isArray(row.events) ||
		row.events.length > 16
	)
		throw new Error("Invalid authentication response");
	const events = row.events.map((value): Event => {
		const event = record(value);
		if (typeof event.type !== "string" || typeof event.id !== "string")
			throw new Error("Invalid authentication event");
		const result: Event = { id: event.id, type: event.type };
		for (const key of ["message", "instructions", "userCode"] as const)
			if (typeof event[key] === "string") result[key] = event[key];
		result.url = safeLoginUrl(event.url);
		result.verificationUri = safeLoginUrl(event.verificationUri);
		if (Array.isArray(event.links))
			result.links = event.links.slice(0, 8).flatMap((value) => {
				const item = record(value);
				const url = safeLoginUrl(item.url);
				return url
					? [
							{
								url,
								label: typeof item.label === "string" ? item.label : "Open",
							},
						]
					: [];
			});
		return result;
	});
	let prompt: Prompt | null = null;
	if (row.prompt) {
		const item = record(row.prompt);
		if (
			typeof item.id !== "string" ||
			typeof item.message !== "string" ||
			!["text", "secret", "manual_code", "select"].includes(String(item.type))
		)
			throw new Error("Invalid authentication prompt");
		prompt = {
			id: item.id,
			message: item.message,
			type: item.type as Prompt["type"],
		};
		if (prompt.type === "select") {
			if (!Array.isArray(item.options) || item.options.length > 32)
				throw new Error("Invalid authentication choices");
			prompt.options = item.options.map((value) => {
				const option = record(value);
				if (typeof option.id !== "string" || typeof option.label !== "string")
					throw new Error("Invalid authentication choice");
				return { id: option.id, label: option.label };
			});
		}
	}
	return {
		id: row.id,
		provider: row.provider,
		state: row.state,
		events,
		prompt,
	};
}
function PrivatePrompt({
	prompt,
	disabled,
	submit,
}: { prompt: Prompt; disabled: boolean; submit: (value: string) => void }) {
	const { t } = useTranslation();
	const [value, setValue] = useState("");
	return (
		<form
			className="space-y-3"
			onSubmit={(event) => {
				event.preventDefault();
				const answer = value;
				setValue("");
				submit(answer);
			}}
		>
			<label className="block text-sm" htmlFor={`login-${prompt.id}`}>
				{prompt.message}
			</label>
			{prompt.type === "select" ? (
				<select
					id={`login-${prompt.id}`}
					className="w-full rounded border bg-background p-2"
					value={value}
					disabled={disabled}
					onChange={(event) => setValue(event.target.value)}
				>
					<option value="" disabled>
						{t("oqtoUi.providerLogin.choose")}
					</option>
					{prompt.options?.map((option) => (
						<option key={option.id} value={option.id}>
							{option.label}
						</option>
					))}
				</select>
			) : (
				<Input
					id={`login-${prompt.id}`}
					type={prompt.type === "text" ? "text" : "password"}
					value={value}
					onChange={(event) => setValue(event.target.value)}
					autoComplete="off"
					spellCheck={false}
					disabled={disabled}
				/>
			)}
			<Button type="submit" disabled={disabled || !value}>
				{t("oqtoUi.providerLogin.continue")}
			</Button>
		</form>
	);
}
/** Mount with an Account/deployment/machine key. Private state never enters query/history caches. */
export function MachineProviders({
	label,
	port,
	close,
}: { label: string; port: ProviderLoginPort; close: () => void }) {
	const { t } = useTranslation();
	const [providers, setProviders] = useState<Provider[]>([]);
	const [search, setSearch] = useState("");
	const [attempt, setAttempt] = useState<Attempt | null>(null);
	const [error, setError] = useState("");
	const [busy, setBusy] = useState(false);
	const lifecycle = useRef({
		alive: false,
		attempt: "",
		sequence: 0,
		applied: 0,
		controller: new AbortController(),
	});
	async function perform(command: LoginCommand) {
		const life = lifecycle.current;
		const sequence = ++life.sequence;
		const result = await port.call(command, life.controller.signal);
		if (!life.alive || sequence < life.applied) return;
		life.applied = sequence;
		if (command.command === "providers") {
			if (!Array.isArray(result)) throw new Error("Invalid provider catalog");
			setProviders(
				result.map((value) => {
					const row = record(value);
					if (
						typeof row.id !== "string" ||
						typeof row.name !== "string" ||
						!Array.isArray(row.methods) ||
						!row.methods.every(
							(method) => method === "api_key" || method === "oauth",
						)
					)
						throw new Error("Invalid provider catalog");
					return {
						id: row.id,
						name: row.name,
						configured: row.configured === true,
						methods: row.methods,
					};
				}),
			);
		} else {
			const next = parseLoginAttempt(result);
			setAttempt(next);
			if (next.state === "saved" || next.state === "saved_refresh_required") {
				setProviders((previous) =>
					previous.map((provider) =>
						provider.id === next.provider
							? { ...provider, configured: true }
							: provider,
					),
				);
			}
			life.attempt = terminal.has(next.state) ? "" : next.id;
		}
		setError("");
	}
	async function act(command: LoginCommand) {
		setBusy(true);
		try {
			await perform(command);
		} catch {
			if (lifecycle.current.alive) setError("unavailable");
		} finally {
			if (lifecycle.current.alive) setBusy(false);
		}
	}
	useMountEffect(() => {
		const life = lifecycle.current;
		life.alive = true;
		life.controller = new AbortController();
		let timer: ReturnType<typeof setTimeout>;
		void act({ command: "providers" });
		const poll = async () => {
			if (!life.alive) return;
			if (life.attempt) {
				try {
					await perform({ command: "status", attempt: life.attempt });
				} catch {
					if (life.alive) {
						setAttempt(null);
						setError("disconnected");
					}
				}
			}
			if (life.alive)
				timer = setTimeout(() => {
					void poll();
				}, 1000);
		};
		timer = setTimeout(() => {
			void poll();
		}, 1000);
		return () => {
			life.alive = false;
			clearTimeout(timer);
			life.controller.abort();
			if (life.attempt)
				void port
					.call({ command: "cancel", attempt: life.attempt })
					.catch(() => {});
		};
	});
	const active = Boolean(lifecycle.current.attempt);
	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open) close();
			}}
		>
			<DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
				<DialogHeader>
					<DialogTitle>
						{t("oqtoUi.providerLogin.title", { machine: label })}
					</DialogTitle>
					<DialogDescription>
						{t("oqtoUi.providerLogin.description")}
					</DialogDescription>
				</DialogHeader>
				{error && (
					<p role="alert" className="text-sm text-destructive">
						{t(`oqtoUi.providerLogin.${error}`)}
					</p>
				)}
				{!active && (
					<Input
						aria-label={t("oqtoUi.providerLogin.search")}
						placeholder={t("oqtoUi.providerLogin.search")}
						value={search}
						onChange={(event) => setSearch(event.target.value)}
					/>
				)}
				{!active && (
					<ul className="space-y-3">
						{providers
							.filter(
								(provider) =>
									provider.methods.length > 0 &&
									`${provider.name} ${provider.id}`
										.toLowerCase()
										.includes(search.trim().toLowerCase()),
							)
							.map((provider) => (
								<li
									key={provider.id}
									className="flex flex-wrap items-center gap-2 border-b py-2"
								>
									<span className="mr-auto text-sm font-medium">
										{provider.name}
										{provider.configured
											? t("oqtoUi.providerLogin.configured")
											: ""}
									</span>
									{provider.methods.map((method) => (
										<Button
											key={method}
											size="sm"
											variant="outline"
											aria-label={`${provider.name}: ${t(method === "oauth" ? "oqtoUi.providerLogin.signIn" : "oqtoUi.providerLogin.apiKey")}`}
											disabled={busy}
											onClick={() =>
												void act({
													command: "start",
													provider: provider.id,
													method,
												})
											}
										>
											{t(
												method === "oauth"
													? "oqtoUi.providerLogin.signIn"
													: "oqtoUi.providerLogin.apiKey",
											)}
										</Button>
									))}
								</li>
							))}
					</ul>
				)}
				{attempt && (
					<section
						aria-label={t("oqtoUi.providerLogin.progress")}
						className="space-y-4"
					>
						<output className="block text-sm">
							{t(`oqtoUi.providerLogin.state.${attempt.state}`)}
						</output>
						{attempt.events.map((event) => (
							<div key={event.id} className="space-y-2 text-sm">
								{event.message && <p>{event.message}</p>}
								{event.instructions && <p>{event.instructions}</p>}
								{event.userCode && (
									<code className="block select-all rounded border p-3 text-center text-lg tracking-widest">
										{event.userCode}
									</code>
								)}
								{(event.url || event.verificationUri) && (
									<a
										className="underline"
										href={event.url || event.verificationUri}
										target="_blank"
										rel="noopener noreferrer"
										referrerPolicy="no-referrer"
									>
										{t("oqtoUi.providerLogin.open")}
									</a>
								)}
								{event.links?.map((link) => (
									<a
										key={link.url}
										className="block underline"
										href={link.url}
										target="_blank"
										rel="noopener noreferrer"
										referrerPolicy="no-referrer"
									>
										{link.label}
									</a>
								))}
							</div>
						))}
						{attempt.prompt && (
							<PrivatePrompt
								key={attempt.prompt.id}
								prompt={attempt.prompt}
								disabled={busy || attempt.state !== "running"}
								submit={(value) => {
									if (attempt.prompt)
										void act({
											command: "answer",
											attempt: attempt.id,
											prompt: attempt.prompt.id,
											value,
										});
								}}
							/>
						)}
					</section>
				)}
				{active && (
					<Button
						variant="outline"
						disabled={busy}
						onClick={() =>
							void act({
								command: "cancel",
								attempt: lifecycle.current.attempt,
							})
						}
					>
						{t("oqtoUi.providerLogin.cancel")}
					</Button>
				)}
				<p className="text-xs text-muted-foreground">
					{t("oqtoUi.providerLogin.callbackHint")}
				</p>
			</DialogContent>
		</Dialog>
	);
}
