import { zodResolver } from "@hookform/resolvers/zod";
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
	const [debugInfo, setDebugInfo] = useState<string | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [isTesting, setIsTesting] = useState(false);

	const form = useForm<LoginFormData>({
		resolver: zodResolver(loginSchema),
		defaultValues: {
			username: "",
			password: "",
			backendUrl: getControlPlaneBaseUrl(),
		},
	});

	async function testConnection() {
		setIsTesting(true);
		setDebugInfo(null);
		setError(null);

		const backendUrl = form.getValues("backendUrl").trim();
		const username = form.getValues("username");
		const password = form.getValues("password");

		if (!backendUrl) {
			setError("Enter a backend URL first");
			setIsTesting(false);
			return;
		}

		setControlPlaneBaseUrl(backendUrl);

		try {
			// Test 1: Features endpoint
			setDebugInfo("1. Testing features...");
			const featuresUrl = `${backendUrl}/features`;
			const featuresRes = await fetch(featuresUrl);
			setDebugInfo(
				`1. Features: ${featuresRes.status} ${featuresRes.ok ? "OK" : "FAIL"}`,
			);

			// Test 2: Login if credentials provided
			if (username && password) {
				setDebugInfo((prev) => `${prev}\n2. Testing login...`);
				const loginUrl = `${backendUrl}/auth/login`;
				const loginRes = await fetch(loginUrl, {
					method: "POST",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ username, password }),
				});
				const loginData = await loginRes.json();
				setDebugInfo(
					(prev) =>
						`${prev}\n2. Login: ${loginRes.status}, token: ${loginData.token ? "YES" : "NO"}`,
				);

				if (loginData.token) {
					// Test 3: /me with token
					setDebugInfo((prev) => `${prev}\n3. Testing /me with token...`);
					const meRes = await fetch(`${backendUrl}/me`, {
						headers: { Authorization: `Bearer ${loginData.token}` },
					});
					const meData = await meRes.json();
					setDebugInfo(
						(prev) =>
							`${prev}\n3. /me: ${meRes.status}, data: ${JSON.stringify(meData).slice(0, 100)}`,
					);
				}
			} else {
				setDebugInfo((prev) => `${prev}\n2. Skipped login (no credentials)`);
			}
		} catch (err) {
			setError(
				`Test failed: ${err instanceof Error ? err.message : String(err)}`,
			);
		} finally {
			setIsTesting(false);
		}
	}

	async function onSubmit(data: LoginFormData) {
		setError(null);
		setDebugInfo(null);
		setIsLoading(true);

		const backendUrl = data.backendUrl.trim();
		setControlPlaneBaseUrl(backendUrl ? backendUrl : null);

		try {
			setDebugInfo("Logging in...");
			const result = await login({
				username: data.username,
				password: data.password,
			});
			setDebugInfo(
				`Login success! Token: ${result.token ? "yes" : "no"}, User: ${result.user?.name || "?"}`,
			);

			// Invalidate auth cache so RequireAuth refetches /me with the new token
			await queryClient.invalidateQueries({ queryKey: authKeys.all });

			setDebugInfo(`Navigating to: ${redirectTo}`);
			navigate(redirectTo, { replace: true });
		} catch (err) {
			const msg = err instanceof Error ? err.message : "Login failed";
			setError(msg);
			setDebugInfo(`Error: ${msg}`);
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

						<FormField
							control={form.control}
							name="backendUrl"
							render={({ field }) => (
								<FormItem>
									<FormLabel>Backend URL</FormLabel>
									<FormControl>
										<Input
											placeholder="http://localhost:8080"
											autoComplete="url"
											disabled={isLoading}
											{...field}
										/>
									</FormControl>
									<FormMessage />
								</FormItem>
							)}
						/>

						<Button
							type="button"
							variant="outline"
							className="w-full"
							disabled={isTesting}
							onClick={testConnection}
						>
							{isTesting ? "Testing..." : "Test Connection"}
						</Button>

						{debugInfo && (
							<pre className="text-xs bg-muted p-2 rounded overflow-auto max-h-32">
								{debugInfo}
							</pre>
						)}

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
