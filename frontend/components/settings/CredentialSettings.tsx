import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useMountEffect } from "@/hooks/use-mount-effect";
import {
	canProvisionCredentials,
	getCredentialStatus,
	manageCredentials,
} from "@/lib/api/credentials";
import type { CredentialCommand } from "@/src/generated/CredentialCommand";
import type { CredentialManageResult } from "@/src/generated/CredentialManageResult";
import { type FormEvent, useCallback, useId, useRef, useState } from "react";

/** Host-owned provisioning only. Secret inputs stay in the DOM, never shared state. */
export function CredentialSettings() {
	const [result, setResult] = useState<CredentialManageResult | null>(null);
	const [busy, setBusy] = useState(false);
	const [failed, setFailed] = useState(false);
	const [selected, setSelected] = useState<string | null>(null);
	const form = useRef<HTMLFormElement>(null);
	const inputId = useId();
	const attachForm = useCallback((node: HTMLFormElement | null) => {
		if (!node) form.current?.reset();
		form.current = node;
	}, []);
	const active = useRef(false);
	const pending = useRef(false);
	const controller = useRef<AbortController | null>(null);
	const safe = canProvisionCredentials();
	useMountEffect(() => {
		active.current = true;
		const initial = new AbortController();
		controller.current = initial;
		void getCredentialStatus(initial.signal)
			.then((next) => {
				if (active.current && !initial.signal.aborted) setResult(next);
			})
			.catch(() => {
				if (active.current && !initial.signal.aborted) setFailed(true);
			});
		return () => {
			active.current = false;
			controller.current?.abort();
			form.current?.reset();
		};
	});
	async function run(command: CredentialCommand) {
		if (pending.current) return;
		pending.current = true;
		controller.current?.abort();
		const current = new AbortController();
		controller.current = current;
		setBusy(true);
		setFailed(false);
		try {
			const next = await manageCredentials(command, current.signal);
			if (active.current && !current.signal.aborted) {
				setResult(next);
				setSelected(null);
			}
		} catch {
			if (active.current) {
				setFailed(true);
				setResult(null);
			}
		} finally {
			pending.current = false;
			if (active.current) setBusy(false);
		}
	}
	function submit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		const inputs = event.currentTarget.elements;
		const secret = inputs.namedItem("secret") as HTMLInputElement;
		const value = secret.value;
		secret.value = ""; // Clear synchronously, including rejected/failed submissions.
		if (!safe || busy || !value || !result) return;
		if (result.status === "uninitialized" || result.status === "locked") {
			void run({
				action: result.status === "locked" ? "unlock" : "initialize",
				passphrase: value,
			});
		} else if (result.status === "ready") {
			const id =
				selected ?? (inputs.namedItem("id") as HTMLInputElement).value.trim();
			const revision =
				result.entries.find((entry) => entry.id === selected)?.revision ?? null;
			if (id)
				void run({ action: "put", id, value, expected_revision: revision });
		}
	}
	return (
		<section className="space-y-4" aria-label="Host credentials">
			<h2 className="text-lg font-semibold">Host credentials</h2>
			<p className="text-sm text-muted-foreground">
				Secrets are stored by the host, not in chat or model state. Values
				cannot be read back. The local vault must be unlocked explicitly after
				every host restart. Locking ends access until you unlock it again.
			</p>
			<output>
				{result
					? `Vault: ${result.status}`
					: failed
						? "Credential status unavailable"
						: "Loading credential status…"}
			</output>
			{failed && (
				<p role="alert">
					Credential request failed. Refresh status before retrying.
				</p>
			)}
			{result?.code && (
				<p role="alert">
					The host did not complete the operation. Refresh status before
					retrying.
				</p>
			)}
			{!safe && (
				<p role="alert">
					Secret submission requires HTTPS or loopback HTTP for both this page
					and the host API.
				</p>
			)}
			{result?.status === "unavailable" && (
				<p>
					The host credential provider is unavailable. kyz integration is not
					available here.
				</p>
			)}
			<Button
				type="button"
				disabled={busy}
				onClick={() => void run({ action: "status" })}
			>
				Refresh status
			</Button>
			{result?.status === "ready" && (
				<>
					<Button
						type="button"
						disabled={busy}
						onClick={() => {
							form.current?.reset();
							void run({ action: "lock" });
						}}
					>
						Lock vault
					</Button>
					<ul className="space-y-2">
						{result.entries.map((entry) => (
							<li key={entry.id} className="flex gap-2 items-center">
								<span>
									{entry.id} · revision {entry.revision}
								</span>
								<Button
									type="button"
									disabled={busy}
									onClick={() => {
										form.current?.reset();
										setSelected(entry.id);
									}}
								>
									Update {entry.id}
								</Button>
								<Button
									type="button"
									disabled={busy}
									onClick={() => {
										form.current?.reset();
										void run({
											action: "delete",
											id: entry.id,
											expected_revision: entry.revision,
										});
									}}
								>
									Delete {entry.id}
								</Button>
							</li>
						))}
					</ul>
				</>
			)}
			{result && result.status !== "unavailable" && (
				<form
					ref={attachForm}
					onSubmit={submit}
					autoComplete="off"
					className="space-y-3"
				>
					{result.status === "ready" && (
						<label className="block" htmlFor={`${inputId}-id`}>
							Credential ID
							<Input
								name="id"
								id={`${inputId}-id`}
								required
								key={selected ?? "new"}
								defaultValue={selected ?? ""}
								disabled={busy || selected !== null}
							/>
						</label>
					)}
					<label className="block" htmlFor={`${inputId}-secret`}>
						{result.status === "ready"
							? "New secret value"
							: "Vault passphrase"}
						<Input
							name="secret"
							id={`${inputId}-secret`}
							type="password"
							autoComplete="new-password"
							required
							disabled={busy || !safe}
						/>
					</label>
					<Button type="submit" disabled={busy || !safe}>
						{result.status === "uninitialized"
							? "Initialize vault"
							: result.status === "locked"
								? "Unlock vault"
								: selected
									? "Save update"
									: "Save credential"}
					</Button>
					{selected && (
						<Button
							type="button"
							onClick={() => {
								form.current?.reset();
								setSelected(null);
							}}
						>
							Cancel update
						</Button>
					)}
				</form>
			)}
		</section>
	);
}
