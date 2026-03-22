"use client";

import { CopyButton } from "@/components/data-display/markdown-renderer";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
	type ApiKeyListItem,
	type OAuthLoginResponse,
	type OAuthProviderInfo,
	createApiKey,
	deleteApiKey,
	deleteOAuthProvider,
	listApiKeys,
	listOAuthProviders,
	pollOAuthDevice,
	startOAuthLogin,
	submitOAuthCallback,
} from "@/lib/api";
import { controlPlaneDirectBaseUrl } from "@/lib/api/client";
import { cn } from "@/lib/utils";
import { AlertCircle, Link2, Loader2, Trash2 } from "lucide-react";
import QRCode from "qrcode";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

const OMNI_KEY_NAME = "omni-vanilla";

function formatDate(value?: string | null): string {
	if (!value) return "-";
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) return value;
	return parsed.toLocaleString();
}

function buildExpiryIso(dateStr: string): string {
	if (!dateStr) return "";
	const iso = new Date(`${dateStr}T23:59:59Z`).toISOString();
	return iso;
}

function addDaysIso(days: number): { iso: string; date: string } {
	const now = new Date();
	now.setUTCDate(now.getUTCDate() + days);
	const date = now.toISOString().slice(0, 10);
	return { iso: buildExpiryIso(date), date };
}

export function ApiKeysPanel() {
	const { t } = useTranslation();
	const [keys, setKeys] = useState<ApiKeyListItem[]>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [connecting, setConnecting] = useState(false);
	const [omniLink, setOmniLink] = useState<string | null>(null);
	const [omniQr, setOmniQr] = useState<string | null>(null);
	const [manualName, setManualName] = useState("");
	const [manualExpires, setManualExpires] = useState("");
	const [manualExpiresDate, setManualExpiresDate] = useState("");
	const [manualKey, setManualKey] = useState<string | null>(null);
	const [manualLoading, setManualLoading] = useState(false);
	const [oauthEnabled, setOauthEnabled] = useState(false);
	const [oauthProviders, setOauthProviders] = useState<OAuthProviderInfo[]>([]);
	const [oauthLoading, setOauthLoading] = useState(true);
	const [oauthError, setOauthError] = useState<string | null>(null);
	const [oauthPending, setOauthPending] = useState<
		Record<
			string,
			{
				login: OAuthLoginResponse;
				code: string;
				submitting: boolean;
				polling: boolean;
			}
		>
	>({});
	const [oauthStarting, setOauthStarting] = useState<Record<string, boolean>>(
		{},
	);

	const serverUrl = useMemo(() => {
		return controlPlaneDirectBaseUrl() || window.location.origin;
	}, []);

	const refresh = useCallback(async () => {
		setLoading(true);
		setError(null);
		try {
			const data = await listApiKeys();
			setKeys(data);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to load API keys");
		} finally {
			setLoading(false);
		}
	}, []);

	const refreshOAuth = useCallback(async () => {
		setOauthLoading(true);
		setOauthError(null);
		try {
			const data = await listOAuthProviders();
			setOauthEnabled(data.enabled);
			setOauthProviders(data.providers ?? []);
		} catch (err) {
			setOauthEnabled(false);
			setOauthProviders([]);
			setOauthError(
				err instanceof Error ? err.message : "Failed to load OAuth providers",
			);
		} finally {
			setOauthLoading(false);
		}
	}, []);

	useEffect(() => {
		void refresh();
		void refreshOAuth();
	}, [refresh, refreshOAuth]);

	const handleConnectOmni = useCallback(async () => {
		setConnecting(true);
		setError(null);
		try {
			const created = await createApiKey({ name: OMNI_KEY_NAME });
			const link = `omni://link/oqto-pulse?url=${encodeURIComponent(
				serverUrl,
			)}&key=${encodeURIComponent(created.api_key)}`;
			setOmniLink(link);
			const qr = await QRCode.toDataURL(link);
			setOmniQr(qr);
			await refresh();
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to create API key");
		} finally {
			setConnecting(false);
		}
	}, [refresh, serverUrl]);

	const handleManualCreate = useCallback(async () => {
		const trimmedName = manualName.trim();
		if (!trimmedName) {
			const message = t(
				"settings.apiKeysNameRequired",
				"Key name is required.",
			);
			setError(message);
			toast.error(message);
			return;
		}
		setManualLoading(true);
		setError(null);
		try {
			const created = await createApiKey({
				name: trimmedName,
				expires_at: manualExpires.trim() || undefined,
			});
			setManualKey(created.api_key);
			setManualName("");
			setManualExpires("");
			setManualExpiresDate("");
			toast.success(t("settings.apiKeysCreatedToast", "API key created."));
			await refresh();
		} catch (err) {
			const message =
				err instanceof Error ? err.message : "Failed to create API key";
			setError(message);
			toast.error(message);
		} finally {
			setManualLoading(false);
		}
	}, [manualExpires, manualName, refresh, t]);

	const handleDelete = useCallback(
		async (id: string) => {
			setError(null);
			try {
				await deleteApiKey(id);
				await refresh();
			} catch (err) {
				setError(
					err instanceof Error ? err.message : "Failed to delete API key",
				);
			}
		},
		[refresh],
	);

	const handleOAuthLogin = useCallback(async (providerId: string) => {
		setOauthError(null);
		setOauthStarting((prev) => ({ ...prev, [providerId]: true }));
		try {
			const response = await startOAuthLogin(providerId);
			setOauthPending((prev) => ({
				...prev,
				[providerId]: {
					login: response,
					code: "",
					submitting: false,
					polling: false,
				},
			}));
		} catch (err) {
			const message =
				err instanceof Error ? err.message : "Failed to start OAuth login";
			setOauthError(message);
			toast.error(message);
		} finally {
			setOauthStarting((prev) => ({ ...prev, [providerId]: false }));
		}
	}, []);

	const handleOAuthCodeChange = useCallback(
		(providerId: string, value: string) => {
			setOauthPending((prev) => {
				const current = prev[providerId];
				if (!current) return prev;
				return {
					...prev,
					[providerId]: { ...current, code: value },
				};
			});
		},
		[],
	);

	const handleOAuthSubmit = useCallback(
		async (providerId: string) => {
			const pending = oauthPending[providerId];
			if (!pending || !pending.login.state) {
				const message = t(
					"settings.oauthMissingState",
					"OAuth login state is missing. Please restart the login flow.",
				);
				setOauthError(message);
				toast.error(message);
				return;
			}
			if (!pending.code.trim()) {
				const message = t(
					"settings.oauthCodeRequired",
					"Authorization code is required.",
				);
				setOauthError(message);
				toast.error(message);
				return;
			}
			setOauthPending((prev) => ({
				...prev,
				[providerId]: { ...pending, submitting: true },
			}));
			try {
				await submitOAuthCallback(pending.code.trim(), pending.login.state);
				toast.success(t("settings.oauthConnected", "Provider connected."));
				setOauthPending((prev) => {
					const next = { ...prev };
					delete next[providerId];
					return next;
				});
				await refreshOAuth();
			} catch (err) {
				const message =
					err instanceof Error ? err.message : "Failed to complete OAuth login";
				setOauthError(message);
				toast.error(message);
			} finally {
				setOauthPending((prev) => {
					const current = prev[providerId];
					if (!current) return prev;
					return {
						...prev,
						[providerId]: { ...current, submitting: false },
					};
				});
			}
		},
		[oauthPending, refreshOAuth, t],
	);

	const handleOAuthPoll = useCallback(
		async (providerId: string) => {
			const pending = oauthPending[providerId];
			if (!pending?.login.device_code) {
				return;
			}
			setOauthPending((prev) => ({
				...prev,
				[providerId]: { ...pending, polling: true },
			}));
			try {
				const response = await pollOAuthDevice(
					providerId,
					pending.login.device_code,
				);
				if (response.status === "stored") {
					toast.success(t("settings.oauthConnected", "Provider connected."));
					setOauthPending((prev) => {
						const next = { ...prev };
						delete next[providerId];
						return next;
					});
					await refreshOAuth();
				}
			} catch (err) {
				const message =
					err instanceof Error ? err.message : "Failed to poll OAuth login";
				setOauthError(message);
				toast.error(message);
			} finally {
				setOauthPending((prev) => {
					const current = prev[providerId];
					if (!current) return prev;
					return {
						...prev,
						[providerId]: { ...current, polling: false },
					};
				});
			}
		},
		[oauthPending, refreshOAuth, t],
	);

	const handleOAuthDelete = useCallback(
		async (providerId: string) => {
			setOauthError(null);
			try {
				await deleteOAuthProvider(providerId);
				await refreshOAuth();
			} catch (err) {
				const message =
					err instanceof Error ? err.message : "Failed to disconnect provider";
				setOauthError(message);
				toast.error(message);
			}
		},
		[refreshOAuth],
	);

	return (
		<div className="space-y-4">
			<Card>
				<CardHeader className="space-y-1">
					<CardTitle className="text-base font-semibold">
						{t("settings.apiKeysTitle", "API Keys")}
					</CardTitle>
					<p className="text-xs text-muted-foreground">
						{t(
							"settings.apiKeysDescription",
							"Generate keys for external integrations. Keys are shown only once.",
						)}
					</p>
				</CardHeader>
				<CardContent className="space-y-4">
					<div className="space-y-3">
						<div className="text-xs text-muted-foreground">
							{t("settings.apiKeysCreateTitle", "Create API key")}
						</div>
						<div className="grid gap-2 sm:grid-cols-[1.5fr_1fr_auto]">
							<Input
								value={manualName}
								onChange={(e) => setManualName(e.target.value)}
								placeholder={t("settings.apiKeysNamePlaceholder", "Key name")}
							/>
							<Input
								type="date"
								value={manualExpiresDate}
								onChange={(e) => {
									const value = e.target.value;
									setManualExpiresDate(value);
									setManualExpires(buildExpiryIso(value));
								}}
								placeholder={t(
									"settings.apiKeysExpiresPlaceholder",
									"Expires at (optional)",
								)}
							/>
							<Button
								type="button"
								onClick={handleManualCreate}
								disabled={manualLoading}
							>
								{manualLoading ? (
									<Loader2 className="h-4 w-4 animate-spin" />
								) : (
									t("settings.apiKeysCreateButton", "Create")
								)}
							</Button>
						</div>
						<div className="flex flex-wrap gap-2">
							{[7, 30, 90].map((days) => (
								<Button
									key={days}
									type="button"
									variant="outline"
									onClick={() => {
										const next = addDaysIso(days);
										setManualExpires(next.iso);
										setManualExpiresDate(next.date);
									}}
								>
									{t("settings.apiKeysPresetDays", "{{days}} days", {
										days,
									})}
								</Button>
							))}
							<Button
								type="button"
								variant="ghost"
								onClick={() => {
									setManualExpires("");
									setManualExpiresDate("");
								}}
							>
								{t("settings.apiKeysNoExpiry", "No expiry")}
							</Button>
						</div>
						<p className="text-xs text-muted-foreground">
							{t(
								"settings.apiKeysExpiresHelp",
								"Pick a date or choose a preset.",
							)}
						</p>
						{manualKey && (
							<div className="space-y-2">
								<div className="text-xs text-muted-foreground">
									{t("settings.apiKeysCreatedLabel", "New API key")}
								</div>
								<div className="flex items-center gap-2">
									<Input value={manualKey} readOnly className="text-xs" />
									<CopyButton text={manualKey} />
								</div>
								<p className="text-xs text-muted-foreground">
									{t(
										"settings.apiKeysCreatedHelp",
										"Copy this key now. It will not be shown again.",
									)}
								</p>
							</div>
						)}
					</div>

					<div className="flex flex-col gap-2">
						<Button
							type="button"
							onClick={handleConnectOmni}
							disabled={connecting}
							className="w-full sm:w-auto"
						>
							{connecting ? (
								<Loader2 className="h-4 w-4 animate-spin" />
							) : (
								<Link2 className="h-4 w-4 mr-2" />
							)}
							{t("settings.connectOmni", "Connect to omni")}
						</Button>
						<p className="text-xs text-muted-foreground">
							{t(
								"settings.connectOmniHelp",
								"Creates a fresh omni key and link.",
							)}
						</p>
					</div>

					{omniLink && (
						<div className="grid gap-4 lg:grid-cols-[1fr_200px]">
							<div className="space-y-2">
								<div className="text-xs text-muted-foreground">
									{t("settings.omniLink", "Omni deep link")}
								</div>
								<div className="flex items-center gap-2">
									<Input value={omniLink} readOnly className="text-xs" />
									<CopyButton text={omniLink} />
								</div>
								<p className="text-xs text-muted-foreground">
									{t(
										"settings.omniLinkHelp",
										"Open this link on a device with omni installed.",
									)}
								</p>
							</div>
							<div className="flex items-center justify-center rounded-md border border-dashed border-border p-3">
								{omniQr ? (
									<img
										src={omniQr}
										alt={t("settings.omniQrAlt", "Omni QR code")}
										className="h-40 w-40"
									/>
								) : (
									<span className="text-xs text-muted-foreground">
										{t("settings.omniQrGenerating", "Generating QR...")}
									</span>
								)}
							</div>
						</div>
					)}

					{error && (
						<div className="flex items-center gap-2 text-xs text-destructive">
							<AlertCircle className="h-4 w-4" />
							<span>{error}</span>
						</div>
					)}
				</CardContent>
			</Card>

			{(oauthLoading || oauthEnabled || oauthError) && (
				<Card>
					<CardHeader className="space-y-1">
						<CardTitle className="text-base font-semibold">
							{t("settings.oauthTitle", "Provider logins")}
						</CardTitle>
						<p className="text-xs text-muted-foreground">
							{t(
								"settings.oauthDescription",
								"Connect provider accounts via OAuth.",
							)}
						</p>
					</CardHeader>
					<CardContent className="space-y-3">
						{oauthLoading ? (
							<div className="flex items-center gap-2 text-sm text-muted-foreground">
								<Loader2 className="h-4 w-4 animate-spin" />
								{t("settings.loading", "Loading...")}
							</div>
						) : !oauthEnabled ? (
							<p className="text-sm text-muted-foreground">
								{t(
									"settings.oauthDisabled",
									"OAuth logins are disabled by the administrator.",
								)}
							</p>
						) : oauthProviders.length === 0 ? (
							<p className="text-sm text-muted-foreground">
								{t("settings.oauthEmpty", "No OAuth providers are configured.")}
							</p>
						) : (
							<div className="space-y-3">
								{oauthProviders.map((provider) => {
									const pending = oauthPending[provider.id];
									const isStarting = oauthStarting[provider.id];
									return (
										<div
											key={provider.id}
											className="rounded-md border border-border p-3 space-y-3"
										>
											<div className="flex flex-wrap items-start justify-between gap-2">
												<div>
													<p className="text-sm font-medium">{provider.name}</p>
													<p className="text-xs text-muted-foreground">
														{provider.connected
															? t("settings.oauthConnectedLabel", "Connected")
															: t(
																	"settings.oauthDisconnectedLabel",
																	"Not connected",
																)}
													</p>
												</div>
												<div className="flex items-center gap-2">
													{provider.connected ? (
														<Button
															variant="outline"
															onClick={() => handleOAuthDelete(provider.id)}
														>
															{t("settings.oauthDisconnect", "Disconnect")}
														</Button>
													) : (
														<Button
															onClick={() => handleOAuthLogin(provider.id)}
															disabled={isStarting}
														>
															{isStarting ? (
																<Loader2 className="h-4 w-4 animate-spin" />
															) : (
																t("settings.oauthConnect", "Connect")
															)}
														</Button>
													)}
												</div>
											</div>
											{pending && (
												<div className="space-y-2 text-xs text-muted-foreground">
													<p>{pending.login.instructions}</p>
													{pending.login.auth_url && (
														<Button
															variant="outline"
															onClick={() =>
																window.open(
																	pending.login.auth_url || "",
																	"_blank",
																)
															}
														>
															{t("settings.oauthOpenLogin", "Open login")}
														</Button>
													)}
													{pending.login.verification_uri &&
														pending.login.user_code && (
															<div className="space-y-1">
																<p>
																	{t(
																		"settings.oauthDeviceUrl",
																		"Verification URL",
																	)}
																	: {pending.login.verification_uri}
																</p>
																<div className="flex items-center gap-2">
																	<Input
																		value={pending.login.user_code}
																		readOnly
																		className="text-xs"
																	/>
																	<CopyButton text={pending.login.user_code} />
																</div>
															</div>
														)}
													{pending.login.state && (
														<div className="space-y-2">
															<Input
																value={pending.code}
																onChange={(e) =>
																	handleOAuthCodeChange(
																		provider.id,
																		e.target.value,
																	)
																}
																placeholder={t(
																	"settings.oauthCodePlaceholder",
																	"Paste authorization code",
																)}
															/>
															<Button
																onClick={() => handleOAuthSubmit(provider.id)}
																disabled={pending.submitting}
															>
																{pending.submitting ? (
																	<Loader2 className="h-4 w-4 animate-spin" />
																) : (
																	t("settings.oauthSubmit", "Submit")
																)}
															</Button>
														</div>
													)}
													{pending.login.device_code && (
														<Button
															variant="outline"
															onClick={() => handleOAuthPoll(provider.id)}
															disabled={pending.polling}
														>
															{pending.polling ? (
																<Loader2 className="h-4 w-4 animate-spin" />
															) : (
																t("settings.oauthCheckStatus", "Check status")
															)}
														</Button>
													)}
												</div>
											)}
										</div>
									);
								})}
							</div>
						)}
						{oauthError && (
							<div className="flex items-center gap-2 text-xs text-destructive">
								<AlertCircle className="h-4 w-4" />
								<span>{oauthError}</span>
							</div>
						)}
					</CardContent>
				</Card>
			)}

			<Card>
				<CardHeader>
					<CardTitle className="text-base font-semibold">
						{t("settings.apiKeysList", "Your keys")}
					</CardTitle>
				</CardHeader>
				<CardContent>
					{loading ? (
						<div className="flex items-center gap-2 text-sm text-muted-foreground">
							<Loader2 className="h-4 w-4 animate-spin" />
							{t("settings.loading", "Loading...")}
						</div>
					) : keys.length === 0 ? (
						<p className="text-sm text-muted-foreground">
							{t("settings.apiKeysEmpty", "No API keys yet.")}
						</p>
					) : (
						<div className="space-y-3">
							{keys.map((key) => (
								<div
									key={key.id}
									className={cn(
										"flex flex-col gap-2 rounded-md border border-border p-3",
										key.revoked_at && "opacity-60",
									)}
								>
									<div className="flex items-start justify-between gap-2">
										<div>
											<p className="text-sm font-medium">{key.name}</p>
											<p className="text-xs text-muted-foreground">
												{t("settings.apiKeyPrefix", "Prefix")}: octo_sk_
												{key.key_prefix}…
											</p>
										</div>
										<Button
											type="button"
											variant="ghost"
											size="icon"
											className="text-destructive hover:text-destructive"
											onClick={() => handleDelete(key.id)}
										>
											<Trash2 className="h-4 w-4" />
										</Button>
									</div>
									<div className="grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
										<span>
											{t("settings.apiKeyCreated", "Created")}:{" "}
											{formatDate(key.created_at)}
										</span>
										<span>
											{t("settings.apiKeyLastUsed", "Last used")}:{" "}
											{formatDate(key.last_used_at)}
										</span>
										<span>
											{t("settings.apiKeyExpires", "Expires")}:{" "}
											{formatDate(key.expires_at)}
										</span>
										<span>
											{t("settings.apiKeyRevoked", "Revoked")}:{" "}
											{formatDate(key.revoked_at)}
										</span>
									</div>
								</div>
							))}
						</div>
					)}
				</CardContent>
			</Card>
		</div>
	);
}
