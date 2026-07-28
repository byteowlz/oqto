import { zodResolver } from "@hookform/resolvers/zod";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { z } from "zod";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardFooter,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import {
	Form,
	FormControl,
	FormField,
	FormItem,
	FormLabel,
	FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { authKeys } from "@/hooks/use-auth";
import {
	getControlPlaneBaseUrl,
	login,
	setControlPlaneBaseUrl,
} from "@/lib/control-plane-client";
import { isTauri } from "@/lib/tauri-fetch-polyfill";
import { useQueryClient } from "@tanstack/react-query";

/**
 * Detect the backend API URL.
 *
 * Priority:
 *   1. Previously stored value in localStorage
 *   2. VITE_CONTROL_PLANE_URL build-time env
 *   3. For Tauri: empty (user must provide)
 *   4. For browser: current origin + "/api" (works behind Caddy reverse proxy)
 */
function detectBackendUrl(): string {
	const stored = getControlPlaneBaseUrl();
	if (stored) return stored;
	// In Tauri there's no reverse proxy -- user must configure
	if (isTauri()) return "";
	// In dev mode (Vite), the dev server proxies /api to the backend.
	// Don't set a base URL — relative paths work through the proxy.
	if (import.meta.env.DEV) return "";
	// Production: same-origin /api works behind Caddy/reverse proxy
	return "";
}

type LoginFormData = {
	username: string;
	password: string;
	backendUrl: string;
};

export function LoginPage() {
	const navigate = useNavigate();
	const { t } = useTranslation();
	const loginSchema = useMemo(
		() =>
			z.object({
				username: z.string().min(1, t("auth.usernameRequired")),
				password: z.string().min(1, t("auth.passwordRequired")),
				backendUrl: z
					.string()
					.trim()
					.url(t("auth.invalidUrl"))
					.or(z.literal("")),
			}),
		[t],
	);
	const authInputClass =
		"border-sidebar-border bg-sidebar-accent/50 shadow-none focus-visible:ring-0 focus-visible:border-primary/50";
	const queryClient = useQueryClient();
	const [searchParams] = useSearchParams();
	const redirectTo = searchParams.get("redirect") || "/";
	const [error, setError] = useState<string | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	// Show backend URL field by default in Tauri (no reverse proxy) or when
	// nothing was auto-detected
	const [showAdvanced, setShowAdvanced] = useState(isTauri());

	const form = useForm<LoginFormData>({
		resolver: zodResolver(loginSchema),
		defaultValues: {
			username: "",
			password: "",
			backendUrl: detectBackendUrl(),
		},
	});

	async function onSubmit(data: LoginFormData) {
		setError(null);
		setIsLoading(true);

		const backendUrl = data.backendUrl.trim();
		setControlPlaneBaseUrl(backendUrl ? backendUrl : null);

		try {
			const result = await login({
				username: data.username,
				password: data.password,
			});

			// Seed the auth cache to avoid a redirect loop while /me refreshes
			queryClient.setQueryData(authKeys.me(), result.user);
			await queryClient.invalidateQueries({ queryKey: authKeys.all });

			navigate(redirectTo, { replace: true });
		} catch (err) {
			const msg = err instanceof Error ? err.message : t("auth.loginFailed");
			setError(msg);
		} finally {
			setIsLoading(false);
		}
	}

	return (
		<Card>
			<CardHeader className="space-y-1">
				<CardTitle className="text-2xl">{t("auth.signIn")}</CardTitle>
				<CardDescription>{t("auth.signInDescription")}</CardDescription>
			</CardHeader>
			<CardContent>
				<Form {...form}>
					<form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
						{error && (
							<Alert variant="destructive">
								<AlertDescription>{error}</AlertDescription>
							</Alert>
						)}

						<FormField
							control={form.control}
							name="username"
							render={({ field }) => (
								<FormItem>
									<FormLabel>{t("auth.username")}</FormLabel>
									<FormControl>
										<Input
											placeholder={t("auth.usernamePlaceholder")}
											autoComplete="username"
											disabled={isLoading}
											className={authInputClass}
											{...field}
										/>
									</FormControl>
									<FormMessage />
								</FormItem>
							)}
						/>

						<FormField
							control={form.control}
							name="password"
							render={({ field }) => (
								<FormItem>
									<FormLabel>{t("auth.password")}</FormLabel>
									<FormControl>
										<Input
											type="password"
											placeholder={t("auth.passwordPlaceholder")}
											autoComplete="current-password"
											disabled={isLoading}
											className={authInputClass}
											{...field}
										/>
									</FormControl>
									<FormMessage />
								</FormItem>
							)}
						/>

						<div>
							<button
								type="button"
								className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
								onClick={() => setShowAdvanced((v) => !v)}
							>
								{showAdvanced ? (
									<ChevronDown className="h-3 w-3" />
								) : (
									<ChevronRight className="h-3 w-3" />
								)}
								{t("auth.advanced")}
							</button>
							{showAdvanced && (
								<div className="mt-2">
									<FormField
										control={form.control}
										name="backendUrl"
										render={({ field }) => (
											<FormItem>
												<FormLabel>{t("auth.backendUrl")}</FormLabel>
												<FormControl>
													<Input
														placeholder="https://your-server.com/api"
														autoComplete="url"
														disabled={isLoading}
														className={authInputClass}
														{...field}
													/>
												</FormControl>
												<FormMessage />
											</FormItem>
										)}
									/>
								</div>
							)}
						</div>

						<Button type="submit" className="w-full" disabled={isLoading}>
							{isLoading ? t("auth.signingIn") : t("auth.signIn")}
						</Button>
					</form>
				</Form>
			</CardContent>
			<CardFooter className="flex flex-col space-y-2">
				<div className="text-sm text-muted-foreground">
					{t("auth.noAccount")}{" "}
					<Link to="/register" className="text-primary hover:underline">
						{t("auth.register")}
					</Link>
				</div>
			</CardFooter>
		</Card>
	);
}
