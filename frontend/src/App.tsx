import { Component, type ReactNode, Suspense, lazy } from "react";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import { AuthLayout } from "./routes/AuthLayout";
import { LoginPage } from "./routes/LoginPage";
import { RegisterPage } from "./routes/RegisterPage";
import { RequireAuth } from "./routes/RequireAuth";

const CHUNK_RELOAD_KEY = "oqto-chunk-reload";

class LazyRouteBoundary extends Component<
	{ children: ReactNode },
	{ failed: boolean }
> {
	state = { failed: false };

	static getDerivedStateFromError() {
		return { failed: true };
	}

	componentDidCatch(error: Error) {
		// A stale deploy/HMR invalidates hashed chunks; one forced reload
		// fetches the fresh index instead of leaving a blank screen
		const chunkFailure =
			/dynamically imported module|Loading chunk|import/i.test(error.message);
		if (chunkFailure && sessionStorage.getItem(CHUNK_RELOAD_KEY) === null) {
			sessionStorage.setItem(CHUNK_RELOAD_KEY, "1");
			window.location.reload();
		}
	}

	render() {
		if (this.state.failed) {
			return (
				<div className="flex h-screen w-screen flex-col items-center justify-center gap-3 bg-background text-foreground">
					<p className="text-sm">This page failed to load.</p>
					<button
						type="button"
						className="border border-border px-3 py-1.5 text-sm hover:bg-muted"
						onClick={() => {
							sessionStorage.removeItem(CHUNK_RELOAD_KEY);
							window.location.reload();
						}}
					>
						Reload
					</button>
				</div>
			);
		}
		return this.props.children;
	}
}

function markChunkLoadSucceeded<T>(module: T): T {
	sessionStorage.removeItem(CHUNK_RELOAD_KEY);
	return module;
}

// Delayed via CSS so fast chunk loads never flash a splash; only slow
// loads (cold cache, stale deploy) ever show it
const routeFallback = (
	<div className="flex h-screen w-screen items-center justify-center bg-background text-sm text-muted-foreground">
		<span
			style={{
				opacity: 0,
				animation: "oqto-route-fallback-in 200ms ease-out 400ms forwards",
			}}
		>
			Loading…
		</span>
		<style>{"@keyframes oqto-route-fallback-in { to { opacity: 1; } }"}</style>
	</div>
);

const AppShellRoute = lazy(() =>
	import("./routes/AppShellRoute").then((module) =>
		markChunkLoadSucceeded({ default: module.AppShellRoute }),
	),
);
const OqtoUiRoute = lazy(() =>
	import("./oqto-ui/app/OqtoUiRoute").then(markChunkLoadSucceeded),
);
const DevOqtoUiRoute = import.meta.env.DEV
	? lazy(() =>
			import("./oqto-ui/app/DevOqtoUiRoute").then(markChunkLoadSucceeded),
		)
	: null;

export function App() {
	return (
		<BrowserRouter
			future={{
				v7_startTransition: true,
				v7_relativeSplatPath: true,
			}}
		>
			<Routes>
				<Route
					path="/oqto-ui"
					element={
						<RequireAuth>
							<LazyRouteBoundary>
								<Suspense fallback={routeFallback}>
									<OqtoUiRoute />
								</Suspense>
							</LazyRouteBoundary>
						</RequireAuth>
					}
				/>
				{DevOqtoUiRoute ? (
					<Route
						path="/dev/oqto-ui"
						element={
							<RequireAuth>
								<LazyRouteBoundary>
									<Suspense fallback={routeFallback}>
										<DevOqtoUiRoute />
									</Suspense>
								</LazyRouteBoundary>
							</RequireAuth>
						}
					/>
				) : null}
				<Route
					path="/login"
					element={
						<AuthLayout>
							<LoginPage />
						</AuthLayout>
					}
				/>
				<Route
					path="/register"
					element={
						<AuthLayout>
							<RegisterPage />
						</AuthLayout>
					}
				/>
				<Route
					path="/*"
					element={
						<RequireAuth>
							<LazyRouteBoundary>
								<Suspense fallback={routeFallback}>
									<AppShellRoute />
								</Suspense>
							</LazyRouteBoundary>
						</RequireAuth>
					}
				/>
			</Routes>
		</BrowserRouter>
	);
}
