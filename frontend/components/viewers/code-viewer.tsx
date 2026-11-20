"use client";

import { getSyntaxLanguage } from "@/lib/file-types";
import { cn } from "@/lib/utils";
import { Check, Copy, FileCode } from "lucide-react";
import { useCallback, useState } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import { ViewerHeader } from "./viewer-header";
import "@/lib/syntax-theme.css";

const TOKEN_THEME_STYLE: Record<string, React.CSSProperties> = {};
const CODE_STYLE: React.CSSProperties = {
	margin: 0,
	padding: "1rem",
	background: "var(--background)",
	fontSize: "0.875rem",
	minHeight: "100%",
};
const LINE_NUMBER_STYLE: React.CSSProperties = {
	minWidth: "3em",
	paddingRight: "1em",
	color: "var(--muted-foreground)",
	opacity: 0.5,
};

interface CodeViewerProps {
	content: string;
	filename?: string;
	language?: string;
	showLineNumbers?: boolean;
	className?: string;
	maxHeight?: string;
	/** Token-colored embedding; the default preserves the incumbent viewer. */
	appearance?: "theme";
}

function CopyButton({ text, className }: { text: string; className?: string }) {
	const [copied, setCopied] = useState(false);

	const handleCopy = useCallback(async () => {
		await navigator.clipboard.writeText(text);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
	}, [text]);

	return (
		<button
			type="button"
			onClick={handleCopy}
			className={cn(
				"p-1.5 rounded-md transition-colors",
				"text-muted-foreground hover:text-foreground hover:bg-muted",
				className,
			)}
			title="Copy to clipboard"
			aria-label="Copy to clipboard"
		>
			{copied ? (
				<Check className="w-4 h-4 text-green-500" />
			) : (
				<Copy className="w-4 h-4" />
			)}
		</button>
	);
}

type CodeContentProps = Pick<
	CodeViewerProps,
	"content" | "filename" | "appearance" | "showLineNumbers" | "maxHeight"
> & { language: string };

function CodeContent({
	content,
	filename,
	appearance,
	showLineNumbers,
	maxHeight,
	language,
}: CodeContentProps) {
	return (
		<div
			className="flex-1 overflow-auto"
			style={{ maxHeight }}
			tabIndex={appearance ? 0 : undefined}
			role={appearance ? "region" : undefined}
			aria-label={appearance ? (filename ?? language) : undefined}
		>
			<SyntaxHighlighter
				style={
					appearance
						? TOKEN_THEME_STYLE
						: (oneDark as Record<string, React.CSSProperties>)
				}
				useInlineStyles={!appearance}
				language={language}
				showLineNumbers={showLineNumbers}
				wrapLines
				wrapLongLines
				customStyle={CODE_STYLE}
				lineNumberStyle={LINE_NUMBER_STYLE}
			>
				{content}
			</SyntaxHighlighter>
		</div>
	);
}

export function CodeViewer({
	content,
	filename,
	language,
	showLineNumbers = true,
	className,
	maxHeight = "100%",
	appearance,
}: CodeViewerProps) {
	// Determine language from filename or use provided
	const effectiveLanguage =
		language || (filename ? getSyntaxLanguage(filename) : "text");

	return (
		<div
			className={cn(
				"flex flex-col h-full rounded-lg overflow-hidden border border-border",
				appearance && "oqto-syntax-theme",
				className,
			)}
		>
			<ViewerHeader
				icon={<FileCode className="w-4 h-4 text-muted-foreground" />}
				filename={filename}
				metadata={
					<span className="text-xs text-muted-foreground font-mono">
						{effectiveLanguage}
					</span>
				}
				actions={<CopyButton text={content} />}
			/>

			{/* Code content */}
			<CodeContent
				content={content}
				filename={filename}
				appearance={appearance}
				showLineNumbers={showLineNumbers}
				maxHeight={maxHeight}
				language={effectiveLanguage}
			/>
		</div>
	);
}
