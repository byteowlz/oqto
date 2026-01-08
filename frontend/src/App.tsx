import { BrowserRouter, Route, Routes } from "react-router-dom";
import { AppShellRoute } from "./routes/AppShellRoute";
import { AuthLayout } from "./routes/AuthLayout";
import { LoginPage } from "./routes/LoginPage";
import { RegisterPage } from "./routes/RegisterPage";
import { RequireAuth } from "./routes/RequireAuth";

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
							<AppShellRoute />
						</RequireAuth>
					}
				/>
			</Routes>
		</BrowserRouter>
	);
}
