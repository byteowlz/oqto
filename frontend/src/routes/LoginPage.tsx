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
import {
	getControlPlaneBaseUrl,
	login,
	setControlPlaneBaseUrl,
} from "@/lib/control-plane-client";

const loginSchema = z.object({
	username: z.string().min(1, "Username is required"),
	password: z.string().min(1, "Password is required"),
	backendUrl: z.string().trim().url("Enter a valid URL").or(z.literal("")),
});

type LoginFormData = z.infer<typeof loginSchema>;

export function LoginPage() {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const redirectTo = searchParams.get("redirect") || "/";
	const [error, setError] = useState<string | null>(null);
	const [isLoading, setIsLoading] = useState(false);

	const form = useForm<LoginFormData>({
		resolver: zodResolver(loginSchema),
		defaultValues: {
			username: "",
			password: "",
			backendUrl: getControlPlaneBaseUrl(),
		},
	});

	async function onSubmit(data: LoginFormData) {
		setError(null);
		setIsLoading(true);

		const backendUrl = data.backendUrl.trim();
		setControlPlaneBaseUrl(backendUrl ? backendUrl : null);

		try {
			await login({ username: data.username, password: data.password });
			navigate(redirectTo, { replace: true });
		} catch (err) {
			setError(err instanceof Error ? err.message : "Login failed");
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
