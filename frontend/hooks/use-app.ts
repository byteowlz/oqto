"use client";

import { AppContext } from "@/components/app-context";
import { useContext } from "react";

export function useApp() {
	const ctx = useContext(AppContext);
	if (!ctx) {
		throw new Error("useApp must be used within an AppProvider");
	}
	return ctx;
}
