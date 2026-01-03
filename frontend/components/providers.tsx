import { ThemeProvider } from "@/components/theme-provider";
import { i18n } from "@/lib/i18n";
import { getQueryClient } from "@/lib/query-client";
import { QueryClientProvider } from "@tanstack/react-query";
import type React from "react";
import { I18nextProvider } from "react-i18next";

type ProvidersProps = {
	children: React.ReactNode;
};

export function Providers({ children }: ProvidersProps) {
	const queryClient = getQueryClient();

	return (
		<QueryClientProvider client={queryClient}>
			<I18nextProvider i18n={i18n}>
				<ThemeProvider
					attribute="class"
					defaultTheme="dark"
					enableSystem
					disableTransitionOnChange
				>
					{children}
				</ThemeProvider>
			</I18nextProvider>
		</QueryClientProvider>
	);
}
