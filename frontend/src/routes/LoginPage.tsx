import { zodResolver } from "@hookform/resolvers/zod";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import { useForm } from "react-hook-form";
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
	// Browser: same-origin /api works behind Caddy
	if (typeof window !== "undefined") {
		return `${window.location.origin}/api`;
	}
	return "";
}

const loginSchema = z.object({
	username: z.string().min(1, "Username is required"),
	password: z.string().min(1, "Password is required"),
	backendUrl: z.string().trim().url("Enter a valid URL").or(z.literal("")),
});

type LoginFormData = z.infer<typeof loginSchema>;

export function LoginPage() {
	const navigate = useNavigate();
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
			const msg = err instanceof Error ? err.message : "Login failed";
			setError(msg);
		} finally {
			setIsLoading(false);
		}
	}

	return (
		<Card>
			<CardHeader className="space-y-1">
				<CardTitle className="text-2xl">Sign in</CardTitle>
				<CardDescription>
					Enter your credentials to access your workspace
				</CardDescription>
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
									<FormLabel>Username</FormLabel>
									<FormControl>
										<Input
											placeholder="Enter your username"
											autoComplete="username"
											disabled={isLoading}
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
									<FormLabel>Password</FormLabel>
									<FormControl>
										<Input
											type="password"
											placeholder="Enter your password"
											autoComplete="current-password"
											disabled={isLoading}
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
								Advanced
							</button>
							{showAdvanced && (
								<div className="mt-2">
									<FormField
										control={form.control}
										name="backendUrl"
										render={({ field }) => (
											<FormItem>
												<FormLabel>Backend URL</FormLabel>
												<FormControl>
													<Input
														placeholder="https://your-server.com/api"
														autoComplete="url"
														disabled={isLoading}
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
							{isLoading ? "Signing in..." : "Sign in"}
						</Button>
					</form>
				</Form>
			</CardContent>
			<CardFooter className="flex flex-col space-y-2">
				<div className="text-sm text-muted-foreground">
					Don&apos;t have an account?{" "}
					<Link to="/register" className="text-primary hover:underline">
						Register
					</Link>
				</div>
			</CardFooter>
		</Card>
	);
}
