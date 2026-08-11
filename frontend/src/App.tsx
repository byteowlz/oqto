import { Suspense, lazy } from "react";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import { AuthLayout } from "./routes/AuthLayout";
import { LoginPage } from "./routes/LoginPage";
import { RegisterPage } from "./routes/RegisterPage";
import { RequireAuth } from "./routes/RequireAuth";

const AppShellRoute = lazy(() =>
	import("./routes/AppShellRoute").then((module) => ({
		default: module.AppShellRoute,
	})),
);
const WorkbenchLabRoute = lazy(
	() => import("./workbench/routes/WorkbenchLabRoute"),
);

export function App() {
	return (
		<BrowserRouter
			future={{
				v7_startTransition: true,
				v7_relativeSplatPath: true,
			}}
		>
			<Routes>
				{import.meta.env.DEV ? (
					<Route
						path="/workbench-lab"
						element={
							<RequireAuth>
								<Suspense fallback={null}>
									<WorkbenchLabRoute />
								</Suspense>
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
							<Suspense fallback={null}>
								<AppShellRoute />
							</Suspense>
						</RequireAuth>
					}
				/>
			</Routes>
		</BrowserRouter>
	);
}
