"use client";

import { cn } from "@/lib/utils";
import { Check, Copy } from "lucide-react";
import { useTheme } from "next-themes";
import { memo, useCallback, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import {
	oneDark,
	oneLight,
} from "react-syntax-highlighter/dist/esm/styles/prism";
import remarkGfm from "remark-gfm";

interface MarkdownRendererProps {
	content: string;
	className?: string;
}

const CopyButton = memo(function CopyButton({
	text,
	className,
}: { text: string; className?: string }) {
	const [copied, setCopied] = useState(false);

	const handleCopy = useCallback(async () => {
		try {
			if (navigator.clipboard?.writeText) {
				await navigator.clipboard.writeText(text);
			} else {
				// Fallback for older browsers or non-HTTPS contexts
				const textArea = document.createElement("textarea");
				textArea.value = text;
				textArea.style.position = "fixed";
				textArea.style.left = "-9999px";
				document.body.appendChild(textArea);
				textArea.select();
				document.execCommand("copy");
				document.body.removeChild(textArea);
			}
			setCopied(true);
			setTimeout(() => setCopied(false), 2000);
		} catch {
			// Silently fail if copy doesn't work
		}
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
		>
			{copied ? (
				<Check className="w-4 h-4 text-primary" />
			) : (
				<Copy className="w-4 h-4" />
			)}
		</button>
	);
});

// Code block with theme awareness
const CodeBlockWithTheme = memo(function CodeBlockWithTheme({
	className,
	children,
}: {
	className?: string;
	children: React.ReactNode;
}) {
	const { resolvedTheme } = useTheme();
	const isDarkMode = resolvedTheme === "dark";

	const match = /language-(\w+)/.exec(className || "");
	const codeString = String(children).replace(/\n$/, "");
	const isInline = !match && !codeString.includes("\n");

	if (isInline) {
		return (
			<code
				className="px-1.5 py-0.5 rounded text-sm font-mono"
				style={{
					backgroundColor: "var(--code-bg)",
					color: "var(--code-success)",
				}}
			>
				{children}
			</code>
		);
	}

	return (
		<div
			className="relative group my-3 overflow-hidden"
			style={{ borderColor: "var(--code-border)", borderWidth: "1px" }}
		>
			<div
				className="flex items-center justify-between px-3 py-2"
				style={{
					backgroundColor: "var(--code-bg)",
					borderBottomColor: "var(--code-border)",
					borderBottomWidth: "1px",
				}}
			>
				<span
					className="text-xs font-mono"
					style={{ color: "var(--code-muted)" }}
				>
					{match ? match[1] : "plaintext"}
				</span>
				<CopyButton text={codeString} />
			</div>
			<SyntaxHighlighter
				style={
					(isDarkMode ? oneDark : oneLight) as Record<
						string,
						React.CSSProperties
					>
				}
				language={match ? match[1] : "text"}
				PreTag="div"
				wrapLongLines={true}
				customStyle={{
					margin: 0,
					padding: "1rem",
					backgroundColor: "var(--code-bg)",
					fontSize: "0.75rem",
					overflowX: "hidden",
					wordBreak: "break-word",
				}}
			>
				{codeString}
			</SyntaxHighlighter>
		</div>
	);
});

// remarkPlugins array - defined once
const remarkPlugins = [remarkGfm];

// Define components outside component to avoid recreation on every render
const markdownComponents: Components = {
	code({ className, children }) {
		return (
			<CodeBlockWithTheme className={className}>{children}</CodeBlockWithTheme>
		);
	},
	p({ children }) {
		return <p className="mb-3 last:mb-0 leading-relaxed">{children}</p>;
	},
	h1({ children }) {
		return (
			<h1 className="text-xl font-bold mb-3 mt-4 first:mt-0 text-foreground">
				{children}
			</h1>
		);
	},
	h2({ children }) {
		return (
			<h2 className="text-lg font-bold mb-2 mt-3 first:mt-0 text-foreground">
				{children}
			</h2>
		);
	},
	h3({ children }) {
		return (
			<h3 className="text-base font-semibold mb-2 mt-3 first:mt-0 text-foreground">
				{children}
			</h3>
		);
	},
	h4({ children }) {
		return (
			<h4 className="text-sm font-semibold mb-2 mt-2 first:mt-0 text-foreground">
				{children}
			</h4>
		);
	},
	ul({ children }) {
		return (
			<ul className="list-disc list-inside mb-3 space-y-1 pl-2">{children}</ul>
		);
	},
	ol({ children }) {
		return (
			<ol className="list-decimal list-inside mb-3 space-y-1 pl-2">
				{children}
			</ol>
		);
	},
	li({ children }) {
		return <li className="text-foreground">{children}</li>;
	},
	blockquote({ children }) {
		return (
			<blockquote className="border-l-2 border-primary pl-4 py-1 my-3 text-muted-foreground italic">
				{children}
			</blockquote>
		);
	},
	a({ href, children }) {
		return (
			<a
				href={href}
				target="_blank"
				rel="noopener noreferrer"
				className="text-primary hover:text-primary/80 underline underline-offset-2"
			>
				{children}
			</a>
		);
	},
	table({ children }) {
		return (
			<div className="overflow-x-auto my-3">
				<table className="min-w-full border border-border rounded-lg overflow-hidden">
					{children}
				</table>
			</div>
		);
	},
	thead({ children }) {
		return <thead className="bg-muted">{children}</thead>;
	},
	tbody({ children }) {
		return <tbody className="divide-y divide-border">{children}</tbody>;
	},
	tr({ children }) {
		return <tr className="divide-x divide-border">{children}</tr>;
	},
	th({ children }) {
		return (
			<th className="px-3 py-2 text-left text-sm font-semibold text-foreground">
				{children}
			</th>
		);
	},
	td({ children }) {
		return (
			<td className="px-3 py-2 text-sm text-muted-foreground">{children}</td>
		);
	},
	hr() {
		return <hr className="my-4 border-border" />;
	},
	strong({ children }) {
		return (
			<strong className="font-semibold text-foreground">{children}</strong>
		);
	},
	em({ children }) {
		return <em className="italic text-muted-foreground">{children}</em>;
	},
};

export const MarkdownRenderer = memo(function MarkdownRenderer({
	content,
	className,
}: MarkdownRendererProps) {
	return (
		<div className={cn("markdown-content", className)}>
			<ReactMarkdown
				remarkPlugins={remarkPlugins}
				components={markdownComponents}
			>
				{content}
			</ReactMarkdown>
		</div>
	);
});

export { CopyButton };
