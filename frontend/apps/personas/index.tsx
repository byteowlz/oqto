"use client";

import { useApp } from "@/components/app-context";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getDefaultAvatarUrl, resolveAvatarUrl } from "@/lib/avatar-utils";
import {
	type Persona,
	type WorkspaceSession,
	listPersonas,
	listWorkspaceSessions,
} from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { ChevronLeft, MessageSquare, Plus, Search, User } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

/** Persona with session count for the list view */
type PersonaWithSessions = {
	persona: Persona;
	sessions: WorkspaceSession[];
	lastActive: string | null;
};

export function PersonasApp() {
	const { locale, createNewChatWithPersona } = useApp();
	const navigate = useNavigate();
	const [searchTerm, setSearchTerm] = useState("");
	const [selectedPersonaId, setSelectedPersonaId] = useState<string | null>(
		null,
	);
	const [mobileView, setMobileView] = useState<"list" | "details">("list");
	const [personas, setPersonas] = useState<PersonaWithSessions[]>([]);
	const [loading, setLoading] = useState(true);

	const copy = useMemo(
		() => ({
			de: {
				searchPlaceholder: "Personas durchsuchen",
				createPersona: "Persona erstellen",
				lastActive: "Zuletzt aktiv",
				sessions: "Sitzungen",
				noSessions: "Keine Sitzungen",
				startChat: "Chat starten",
				back: "Zurück",
				selectPersona: "Wähle eine Persona aus der Liste",
				noPersonas: "Keine Personas gefunden",
				description: "Beschreibung",
				recentSessions: "Letzte Sitzungen",
			},
			en: {
				searchPlaceholder: "Search personas",
				createPersona: "Create Persona",
				lastActive: "Last active",
				sessions: "sessions",
				noSessions: "No sessions",
				startChat: "Start Chat",
				back: "Back",
				selectPersona: "Select a persona from the list",
				noPersonas: "No personas found",
				description: "Description",
				recentSessions: "Recent Sessions",
			},
		}),
		[],
	);
	const t = copy[locale];

	// Fetch personas and sessions
	useEffect(() => {
		const fetchData = async () => {
			try {
				// Fetch personas and sessions in parallel
				const [personaList, sessions] = await Promise.all([
					listPersonas(),
					listWorkspaceSessions(),
				]);

				// Create a map of persona ID to sessions
				const sessionsByPersonaId = new Map<string, WorkspaceSession[]>();
				for (const session of sessions) {
					if (session.persona?.id) {
						const existing = sessionsByPersonaId.get(session.persona.id) ?? [];
						existing.push(session);
						sessionsByPersonaId.set(session.persona.id, existing);
					}
				}

				// Build PersonaWithSessions for each persona
				const personasWithSessions: PersonaWithSessions[] = personaList.map(
					(persona) => {
						const personaSessions = sessionsByPersonaId.get(persona.id) ?? [];

						// Find most recent activity
						let lastActive: string | null = null;
						for (const session of personaSessions) {
							const sessionTime = session.started_at || session.created_at;
							if (!lastActive || sessionTime > lastActive) {
								lastActive = sessionTime;
							}
						}

						return {
							persona,
							sessions: personaSessions,
							lastActive,
						};
					},
				);

				// Sort: default persona first, then by last active (most recent first)
				personasWithSessions.sort((a, b) => {
					if (a.persona.is_default && !b.persona.is_default) return -1;
					if (!a.persona.is_default && b.persona.is_default) return 1;
					if (!a.lastActive) return 1;
					if (!b.lastActive) return -1;
					return b.lastActive.localeCompare(a.lastActive);
				});

				setPersonas(personasWithSessions);
			} catch (err) {
				console.error("Failed to fetch personas:", err);
			} finally {
				setLoading(false);
			}
		};

		fetchData();
	}, []);

	const filteredPersonas = personas.filter(
		(p) =>
			p.persona.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
			p.persona.description.toLowerCase().includes(searchTerm.toLowerCase()),
	);

	const selectedPersona = personas.find(
		(p) => p.persona.id === selectedPersonaId,
	);

	const handlePersonaSelect = (id: string) => {
		setSelectedPersonaId(id);
		setMobileView("details");
	};

	const handleStartChat = async () => {
		if (!selectedPersona) return;

		try {
			const session = await createNewChatWithPersona(selectedPersona.persona);
			if (session) {
				// Navigate to sessions page
				navigate("/sessions");
			}
		} catch (err) {
			console.error("Failed to start chat with persona:", err);
		}
	};

	const formatRelativeTime = (dateStr: string | null): string => {
		if (!dateStr) return t.noSessions;

		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffMins = Math.floor(diffMs / 60000);
		const diffHours = Math.floor(diffMins / 60);
		const diffDays = Math.floor(diffHours / 24);

		if (diffMins < 1) return locale === "de" ? "Gerade eben" : "Just now";
		if (diffMins < 60)
			return locale === "de" ? `vor ${diffMins} Min.` : `${diffMins}m ago`;
		if (diffHours < 24)
			return locale === "de" ? `vor ${diffHours} Std.` : `${diffHours}h ago`;
		if (diffDays < 7)
			return locale === "de" ? `vor ${diffDays} Tagen` : `${diffDays}d ago`;
		return date.toLocaleDateString();
	};

	// Get avatar URL for a persona
	const getAvatarUrl = (
		persona: Persona,
		session?: WorkspaceSession,
	): string | null => {
		// Try to resolve the avatar from persona config
		const resolved = resolveAvatarUrl(persona.avatar, session?.id);
		if (resolved) return resolved;

		// Fall back to default avatar based on persona ID
		return getDefaultAvatarUrl(persona.id);
	};

	// Persona List Component
	const PersonaList = (
		<div className="flex flex-col h-full bg-background">
			{/* Search */}
			<div className="p-4">
				<div className="relative">
					<Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-muted-foreground" />
					<Input
						placeholder={t.searchPlaceholder}
						value={searchTerm}
						onChange={(e) => setSearchTerm(e.target.value)}
						className="pl-9 h-10 bg-transparent border-border text-foreground placeholder:text-muted-foreground"
					/>
				</div>
			</div>

			{/* Create Persona Button */}
			<div className="px-4 pb-4">
				<button
					type="button"
					className="w-full flex items-center justify-between px-4 py-3 text-sm text-foreground hover:bg-primary/10 transition-colors border border-dashed border-border"
				>
					<span className="flex items-center gap-2">
						<Plus className="w-4 h-4" />
						{t.createPersona}
					</span>
				</button>
			</div>

			{/* Persona List */}
			<div className="flex-1 overflow-y-auto px-4 space-y-2">
				{loading ? (
					<div className="text-center text-muted-foreground py-8">
						Loading...
					</div>
				) : filteredPersonas.length === 0 ? (
					<div className="text-center text-muted-foreground py-8">
						{t.noPersonas}
					</div>
				) : (
					filteredPersonas.map((item) => {
						const isSelected = selectedPersonaId === item.persona.id;
						const avatarUrl = getAvatarUrl(item.persona, item.sessions[0]);

						return (
							<button
								type="button"
								key={item.persona.id}
								onClick={() => handlePersonaSelect(item.persona.id)}
								className={cn(
									"w-full text-left p-3 transition-colors flex items-start gap-3",
									isSelected
										? "bg-primary text-primary-foreground"
										: "text-foreground hover:bg-primary/10",
								)}
							>
								{/* Avatar */}
								<div
									className="w-10 h-10 rounded-full flex items-center justify-center flex-shrink-0"
									style={{ backgroundColor: item.persona.color || "#6366f1" }}
								>
									{avatarUrl ? (
										<img
											src={avatarUrl}
											alt={item.persona.name}
											className="w-full h-full rounded-full object-cover"
										/>
									) : (
										<User className="w-5 h-5 text-white" />
									)}
								</div>

								<div className="flex-1 min-w-0">
									<div className="font-medium text-sm truncate">
										{item.persona.name}
									</div>
									<div
										className={cn(
											"text-xs mt-0.5 truncate",
											isSelected
												? "text-primary-foreground/70"
												: "text-muted-foreground",
										)}
									>
										{item.persona.description || t.noSessions}
									</div>
									<div
										className={cn(
											"text-xs mt-1 flex items-center gap-2",
											isSelected
												? "text-primary-foreground/70"
												: "text-muted-foreground",
										)}
									>
										<span>
											{item.sessions.length} {t.sessions}
										</span>
										<span>·</span>
										<span>{formatRelativeTime(item.lastActive)}</span>
									</div>
								</div>
							</button>
						);
					})
				)}
			</div>
		</div>
	);

	// Persona Details Component
	const PersonaDetails = (
		<div className="flex flex-col h-full bg-background">
			{selectedPersona ? (
				<>
					{/* Persona Header */}
					<div className="p-4 md:p-6 border-b border-border">
						<div className="flex items-start gap-4">
							<button
								type="button"
								onClick={() => setMobileView("list")}
								className="md:hidden p-2 -ml-2 text-muted-foreground hover:text-foreground"
							>
								<ChevronLeft className="w-5 h-5" />
							</button>

							{/* Large Avatar */}
							<div
								className="w-16 h-16 rounded-full flex items-center justify-center flex-shrink-0"
								style={{
									backgroundColor: selectedPersona.persona.color || "#6366f1",
								}}
							>
								{(() => {
									const avatarUrl = getAvatarUrl(
										selectedPersona.persona,
										selectedPersona.sessions[0],
									);
									return avatarUrl ? (
										<img
											src={avatarUrl}
											alt={selectedPersona.persona.name}
											className="w-full h-full rounded-full object-cover"
										/>
									) : (
										<User className="w-8 h-8 text-white" />
									);
								})()}
							</div>

							<div className="flex-1">
								<h1 className="text-lg md:text-xl font-semibold text-foreground">
									{selectedPersona.persona.name}
								</h1>
								{selectedPersona.persona.description && (
									<p className="text-sm text-muted-foreground mt-1">
										{selectedPersona.persona.description}
									</p>
								)}
								<div className="text-sm text-muted-foreground mt-2">
									{selectedPersona.sessions.length} {t.sessions} ·{" "}
									{t.lastActive}:{" "}
									{formatRelativeTime(selectedPersona.lastActive)}
								</div>
							</div>
						</div>

						<div className="mt-4">
							<Button
								type="button"
								className="w-full md:w-auto"
								style={{
									backgroundColor: selectedPersona.persona.color || undefined,
								}}
								onClick={handleStartChat}
							>
								<MessageSquare className="w-4 h-4 mr-2" />
								{t.startChat}
							</Button>
						</div>
					</div>

					{/* Recent Sessions */}
					<div className="flex-1 overflow-y-auto p-4 md:p-6">
						<h2 className="text-sm font-medium text-muted-foreground mb-4">
							{t.recentSessions}
						</h2>

						<div className="space-y-2">
							{selectedPersona.sessions.slice(0, 10).map((session) => (
								<div
									key={session.id}
									className="border border-border bg-card p-3 hover:bg-muted/50 transition-colors cursor-pointer"
								>
									<div className="flex items-center justify-between">
										<div className="font-medium text-sm">
											{session.readable_id || session.id.slice(0, 8)}
										</div>
										<div
											className={cn(
												"text-xs px-2 py-0.5 rounded",
												session.status === "running"
													? "bg-green-500/20 text-green-500"
													: session.status === "stopped"
														? "bg-muted text-muted-foreground"
														: "bg-yellow-500/20 text-yellow-500",
											)}
										>
											{session.status}
										</div>
									</div>
									<div className="text-xs text-muted-foreground mt-1">
										{formatRelativeTime(
											session.started_at || session.created_at,
										)}
									</div>
								</div>
							))}
						</div>
					</div>
				</>
			) : (
				/* Empty State */
				<div className="flex-1 flex items-center justify-center p-6">
					<div className="text-center text-muted-foreground">
						<User className="w-12 h-12 mx-auto mb-4 opacity-50" />
						<p className="text-sm">{t.selectPersona}</p>
					</div>
				</div>
			)}
		</div>
	);

	return (
		<>
			{/* Mobile Layout - Show one view at a time */}
			<div className="md:hidden h-full w-full overflow-hidden">
				{mobileView === "list" ? PersonaList : PersonaDetails}
			</div>

			{/* Desktop Layout - Side by side */}
			<div className="hidden md:flex h-full w-full overflow-hidden">
				{/* Column 1: Persona List */}
				<div className="w-[320px] min-w-[320px] border-r border-border">
					{PersonaList}
				</div>

				{/* Column 2: Persona Details */}
				<div className="flex-1">{PersonaDetails}</div>
			</div>
		</>
	);
}

export default PersonasApp;
