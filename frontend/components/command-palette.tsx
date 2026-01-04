"use client";

import {
	CommandDialog,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
	CommandSeparator,
	CommandShortcut,
} from "@/components/ui/command";
import { useApp } from "@/hooks/use-app";
import {
	VOICE_SHORTCUTS,
	formatShortcut,
	useVoiceCommandEmitter,
} from "@/hooks/use-voice-commands";
import { generateReadableId } from "@/lib/session-utils";
import {
	AudioLines,
	Bot,
	Cog,
	FolderKanban,
	Globe2,
	Keyboard,
	MessageSquare,
	MoonStar,
	Plus,
	Shield,
	SunMedium,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

interface CommandPaletteProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}

export function CommandPalette({ open, onOpenChange }: CommandPaletteProps) {
	const {
		apps,
		setActiveAppId,
		locale,
		setLocale,
		opencodeSessions,
		setSelectedChatSessionId,
		createNewChat,
	} = useApp();

	const { startConversation, startDictation } = useVoiceCommandEmitter();

	const [theme, setThemeState] = useState<"light" | "dark">("dark");

	useEffect(() => {
		try {
			const stored = localStorage.getItem("theme");
			if (stored === "light" || stored === "dark") {
				setThemeState(stored);
			}
		} catch {
			// Ignore storage failures.
		}
	}, []);

	const toggleTheme = useCallback(() => {
		const next = theme === "dark" ? "light" : "dark";
		document.documentElement.classList.add("no-transitions");
		document.documentElement.classList.toggle("dark", next === "dark");
		try {
			localStorage.setItem("theme", next);
		} catch {
			// Ignore storage failures.
		}
		setThemeState(next);
		requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				document.documentElement.classList.remove("no-transitions");
			});
		});
		onOpenChange(false);
	}, [theme, onOpenChange]);

	const toggleLocale = useCallback(() => {
		const next = locale === "de" ? "en" : "de";
		setLocale(next);
		onOpenChange(false);
	}, [locale, setLocale, onOpenChange]);

	const handleNavigation = useCallback(
		(appId: string) => {
			setActiveAppId(appId);
			onOpenChange(false);
		},
		[setActiveAppId, onOpenChange],
	);

	const handleNewChat = useCallback(async () => {
		await createNewChat();
		setActiveAppId("sessions");
		onOpenChange(false);
	}, [createNewChat, setActiveAppId, onOpenChange]);

	const handleSelectSession = useCallback(
		(sessionId: string) => {
			setSelectedChatSessionId(sessionId);
			setActiveAppId("sessions");
			onOpenChange(false);
		},
		[setSelectedChatSessionId, setActiveAppId, onOpenChange],
	);

	const getAppIcon = (appId: string) => {
		switch (appId) {
			case "projects":
				return FolderKanban;
			case "sessions":
				return MessageSquare;
			case "workspaces":
				return Bot;
			case "admin":
				return Shield;
			case "settings":
				return Cog;
			default:
				return FolderKanban;
		}
	};

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title={locale === "de" ? "Befehlspalette" : "Command Palette"}
			description={
				locale === "de"
					? "Suchen Sie nach einem Befehl..."
					: "Search for a command..."
			}
		>
			<CommandInput
				placeholder={
					locale === "de"
						? "Befehl eingeben oder suchen..."
						: "Type a command or search..."
				}
			/>
			<CommandList>
				<CommandEmpty>
					{locale === "de" ? "Keine Ergebnisse gefunden." : "No results found."}
				</CommandEmpty>

				<CommandGroup heading={locale === "de" ? "Aktionen" : "Actions"}>
					<CommandItem onSelect={handleNewChat}>
						<Plus className="mr-2 h-4 w-4" />
						<span>{locale === "de" ? "Neuer Chat" : "New Chat"}</span>
						<CommandShortcut>N</CommandShortcut>
					</CommandItem>
					<CommandItem onSelect={toggleTheme}>
						{theme === "dark" ? (
							<SunMedium className="mr-2 h-4 w-4" />
						) : (
							<MoonStar className="mr-2 h-4 w-4" />
						)}
						<span>
							{locale === "de"
								? `Zu ${theme === "dark" ? "hellem" : "dunklem"} Modus wechseln`
								: `Switch to ${theme === "dark" ? "light" : "dark"} mode`}
						</span>
					</CommandItem>
					<CommandItem onSelect={toggleLocale}>
						<Globe2 className="mr-2 h-4 w-4" />
						<span>
							{locale === "de"
								? "Sprache wechseln (EN)"
								: "Change language (DE)"}
						</span>
					</CommandItem>
				</CommandGroup>

				<CommandSeparator />

				<CommandGroup heading={locale === "de" ? "Sprache" : "Voice"}>
					<CommandItem
						onSelect={() => {
							startConversation();
							setActiveAppId("sessions");
							onOpenChange(false);
						}}
					>
						<AudioLines className="mr-2 h-4 w-4" />
						<span>
							{locale === "de" ? "Konversation starten" : "Start Conversation"}
						</span>
						<CommandShortcut>
							{formatShortcut(VOICE_SHORTCUTS.conversation)}
						</CommandShortcut>
					</CommandItem>
					<CommandItem
						onSelect={() => {
							startDictation();
							setActiveAppId("sessions");
							onOpenChange(false);
						}}
					>
						<Keyboard className="mr-2 h-4 w-4" />
						<span>
							{locale === "de" ? "Diktat starten" : "Start Dictation"}
						</span>
						<CommandShortcut>
							{formatShortcut(VOICE_SHORTCUTS.dictation)}
						</CommandShortcut>
					</CommandItem>
				</CommandGroup>

				<CommandSeparator />

				<CommandGroup heading={locale === "de" ? "Navigation" : "Navigation"}>
					{apps.map((app) => {
						const Icon = getAppIcon(app.id);
						const label =
							typeof app.label === "string"
								? app.label
								: locale === "en"
									? app.label.en
									: app.label.de;
						return (
							<CommandItem
								key={app.id}
								onSelect={() => handleNavigation(app.id)}
							>
								<Icon className="mr-2 h-4 w-4" />
								<span>{label}</span>
							</CommandItem>
						);
					})}
				</CommandGroup>

				{opencodeSessions.length > 0 && (
					<>
						<CommandSeparator />
						<CommandGroup
							heading={locale === "de" ? "Letzte Chats" : "Recent Chats"}
						>
							{opencodeSessions.slice(0, 5).map((session) => (
								<CommandItem
									key={session.id}
									onSelect={() => handleSelectSession(session.id)}
								>
									<MessageSquare className="mr-2 h-4 w-4" />
									<span className="truncate">
										{session.title || "Untitled"}
									</span>
									<span className="ml-2 text-xs text-muted-foreground font-mono">
										{generateReadableId(session.id)}
									</span>
								</CommandItem>
							))}
						</CommandGroup>
					</>
				)}
			</CommandList>
		</CommandDialog>
	);
}
