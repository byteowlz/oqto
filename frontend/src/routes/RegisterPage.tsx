import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect, useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { Link, useNavigate } from "react-router-dom";
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
import { register } from "@/lib/control-plane-client";
import { useQueryClient } from "@tanstack/react-query";

const registerSchema = z
	.object({
		username: z
			.string()
			.min(3, "Username must be at least 3 characters")
			.max(50, "Username must be at most 50 characters")
			.regex(
				/^[a-zA-Z0-9_-]+$/,
				"Username can only contain letters, numbers, underscores, and hyphens",
			),
		email: z.string().email("Please enter a valid email address"),
		password: z.string().min(6, "Password must be at least 6 characters"),
		confirmPassword: z.string(),
		inviteCode: z.string().min(1, "Invite code is required"),
		displayName: z.string().optional(),
	})
	.refine((data) => data.password === data.confirmPassword, {
		message: "Passwords do not match",
		path: ["confirmPassword"],
	});

type RegisterFormData = z.infer<typeof registerSchema>;

// Provisioning steps shown to the user during account creation.
// These are timed estimates -- the actual backend work is a single API call.
const PROVISIONING_STEPS = [
	{ label: "Creating user account...", delay: 0 },
	{ label: "Setting up workspace environment...", delay: 2000 },
	{ label: "Configuring AI services...", delay: 5000 },
	{ label: "Starting background services...", delay: 9000 },
	{ label: "Verifying service health...", delay: 14000 },
	{ label: "Almost there...", delay: 22000 },
];

function ProvisioningStatus({ startTime }: { startTime: number }) {
	const [currentStep, setCurrentStep] = useState(0);

	useEffect(() => {
		const timers: ReturnType<typeof setTimeout>[] = [];
		for (let i = 1; i < PROVISIONING_STEPS.length; i++) {
			const elapsed = Date.now() - startTime;
			const remaining = PROVISIONING_STEPS[i].delay - elapsed;
			if (remaining > 0) {
				timers.push(setTimeout(() => setCurrentStep(i), remaining));
			} else {
				setCurrentStep(i);
			}
		}
		return () => timers.forEach(clearTimeout);
	}, [startTime]);

	return (
		<div className="space-y-3 py-4">
			{PROVISIONING_STEPS.map((step, idx) => (
				<div
					key={step.label}
					className="flex items-center gap-3 text-sm transition-opacity duration-300"
					style={{ opacity: idx <= currentStep ? 1 : 0.3 }}
				>
					{idx < currentStep ? (
						<svg
							className="h-4 w-4 shrink-0 text-green-500"
							viewBox="0 0 16 16"
							fill="currentColor"
						>
							<path
								fillRule="evenodd"
								d="M13.78 4.22a.75.75 0 010 1.06l-7.25 7.25a.75.75 0 01-1.06 0L2.22 9.28a.75.75 0 011.06-1.06L6 10.94l6.72-6.72a.75.75 0 011.06 0z"
							/>
						</svg>
					) : idx === currentStep ? (
						<svg
							className="h-4 w-4 shrink-0 animate-spin text-primary"
							viewBox="0 0 16 16"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
						>
							<circle cx="8" cy="8" r="6" opacity="0.25" />
							<path d="M14 8a6 6 0 00-6-6" strokeLinecap="round" />
						</svg>
					) : (
						<div className="h-4 w-4 shrink-0" />
					)}
					<span
						className={
							idx === currentStep
								? "text-foreground font-medium"
								: idx < currentStep
									? "text-muted-foreground"
									: "text-muted-foreground/50"
						}
					>
						{step.label}
					</span>
				</div>
			))}
		</div>
	);
}

export function RegisterPage() {
	const navigate = useNavigate();
	const queryClient = useQueryClient();
	const [error, setError] = useState<string | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [isProvisioning, setIsProvisioning] = useState(false);
	const provisioningStartRef = useRef(0);

	const form = useForm<RegisterFormData>({
		resolver: zodResolver(registerSchema),
		defaultValues: {
			username: "",
			email: "",
			password: "",
			confirmPassword: "",
			inviteCode: "",
			displayName: "",
		},
	});

	async function onSubmit(data: RegisterFormData) {
		setError(null);
		setIsLoading(true);
		setIsProvisioning(true);
		provisioningStartRef.current = Date.now();

		try {
			const result = await register({
				username: data.username,
				email: data.email,
				password: data.password,
				invite_code: data.inviteCode,
				display_name: data.displayName || undefined,
			});

			// Seed the auth cache so RequireAuth doesn't redirect to login
			if (result.user) {
				queryClient.setQueryData(authKeys.me(), result.user);
			}
			await queryClient.invalidateQueries({ queryKey: authKeys.all });

			navigate("/", { replace: true });
		} catch (err) {
			const message =
				err instanceof Error ? err.message : "Registration failed";
			// Strip internal error details -- show a user-friendly message
			if (
				message.includes("Internal server error") ||
				message.includes("Failed to create user account")
			) {
				setError(
					"Account setup is taking longer than expected. Please wait a moment and try again. " +
						"If this keeps happening, contact an administrator.",
				);
			} else {
				setError(message);
			}
		} finally {
			setIsLoading(false);
			setIsProvisioning(false);
		}
	}

	return (
		<Card>
			<CardHeader className="space-y-1">
				<CardTitle className="text-2xl">Create an account</CardTitle>
				<CardDescription>
					Enter your details and invite code to get started
				</CardDescription>
			</CardHeader>
			<CardContent>
				{isProvisioning ? (
					<div className="min-h-[200px]">
						<p className="text-sm text-muted-foreground mb-2">
							Setting up your workspace. This can take up to 30 seconds...
						</p>
						<ProvisioningStatus startTime={provisioningStartRef.current} />
					</div>
				) : (
					<Form {...form}>
						<form
							onSubmit={form.handleSubmit(onSubmit)}
							className="space-y-4"
						>
							{error && (
								<Alert variant="destructive">
									<AlertDescription>{error}</AlertDescription>
								</Alert>
							)}

							<FormField
								control={form.control}
								name="inviteCode"
								render={({ field }) => (
									<FormItem>
										<FormLabel>Invite Code</FormLabel>
										<FormControl>
											<Input
												placeholder="Enter your invite code"
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
								name="username"
								render={({ field }) => (
									<FormItem>
										<FormLabel>Username</FormLabel>
										<FormControl>
											<Input
												placeholder="Choose a username"
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
								name="email"
								render={({ field }) => (
									<FormItem>
										<FormLabel>Email</FormLabel>
										<FormControl>
											<Input
												type="email"
												placeholder="Enter your email"
												autoComplete="email"
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
								name="displayName"
								render={({ field }) => (
									<FormItem>
										<FormLabel>Display Name (optional)</FormLabel>
										<FormControl>
											<Input
												placeholder="How should we call you?"
												autoComplete="name"
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
												placeholder="Create a password"
												autoComplete="new-password"
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
								name="confirmPassword"
								render={({ field }) => (
									<FormItem>
										<FormLabel>Confirm Password</FormLabel>
										<FormControl>
											<Input
												type="password"
												placeholder="Confirm your password"
												autoComplete="new-password"
												disabled={isLoading}
												{...field}
											/>
										</FormControl>
										<FormMessage />
									</FormItem>
								)}
							/>

							<Button
								type="submit"
								className="w-full"
								disabled={isLoading}
							>
								{isLoading ? "Creating account..." : "Create account"}
							</Button>
						</form>
					</Form>
				)}
			</CardContent>
			<CardFooter className="flex flex-col space-y-2">
				<div className="text-sm text-muted-foreground">
					Already have an account?{" "}
					<Link to="/login" className="text-primary hover:underline">
						Sign in
					</Link>
				</div>
			</CardFooter>
		</Card>
	);
}
