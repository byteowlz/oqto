"use client";

import {
	BlockTypeSelect,
	BoldItalicUnderlineToggles,
	CreateLink,
	InsertCodeBlock,
	InsertTable,
	ListsToggle,
	MDXEditor,
	type MDXEditorMethods,
	UndoRedo,
	codeBlockPlugin,
	codeMirrorPlugin,
	headingsPlugin,
	imagePlugin,
	linkPlugin,
	listsPlugin,
	markdownShortcutPlugin,
	quotePlugin,
	tablePlugin,
	thematicBreakPlugin,
	toolbarPlugin,
} from "@mdxeditor/editor";
import "@mdxeditor/editor/style.css";
import { useEffect, useRef } from "react";

interface MarkdownWysiwygEditorProps {
	markdown: string;
	onChange: (value: string) => void;
	className?: string;
}

const codeLanguages: Record<string, string> = {
	txt: "Text",
	bash: "Bash",
	json: "JSON",
	yaml: "YAML",
	typescript: "TypeScript",
	javascript: "JavaScript",
	tsx: "TSX",
	jsx: "JSX",
	rust: "Rust",
	python: "Python",
	go: "Go",
	sql: "SQL",
};

function transformOutsideFencedCode(
	markdown: string,
	transformLine: (line: string) => string,
) {
	let inFence = false;
	return markdown
		.split("\n")
		.map((line) => {
			if (/^\s*(```|~~~)/.test(line)) {
				inFence = !inFence;
				return line;
			}
			return inFence ? line : transformLine(line);
		})
		.join("\n");
}

function escapeMdxText(markdown: string) {
	return transformOutsideFencedCode(markdown, (line) =>
		line.replace(/<([^>\n]+)>/g, "&lt;$1&gt;"),
	);
}

function unescapeMdxText(markdown: string) {
	return transformOutsideFencedCode(markdown, (line) =>
		line.replace(/&lt;/g, "<").replace(/&gt;/g, ">"),
	);
}

const editorPlugins = [
	headingsPlugin(),
	listsPlugin(),
	quotePlugin(),
	linkPlugin(),
	imagePlugin(),
	tablePlugin(),
	thematicBreakPlugin(),
	codeBlockPlugin({ defaultCodeBlockLanguage: "txt" }),
	codeMirrorPlugin({ codeBlockLanguages: codeLanguages }),
	markdownShortcutPlugin(),
	toolbarPlugin({
		toolbarContents: () => (
			<>
				<UndoRedo />
				<BlockTypeSelect />
				<BoldItalicUnderlineToggles />
				<ListsToggle />
				<CreateLink />
				<InsertCodeBlock />
				<InsertTable />
			</>
		),
	}),
];

function MarkdownWysiwygEditor({
	markdown,
	onChange,
	className,
}: MarkdownWysiwygEditorProps) {
	const editorRef = useRef<MDXEditorMethods>(null);
	const editorMarkdown = escapeMdxText(markdown);
	const latestMarkdownRef = useRef(editorMarkdown);
	const hasSyncedInitialMarkdownRef = useRef(false);

	// useeffect-guardrail: allow - MDXEditor reads markdown only on mount and lazy/chunk timing can mount before the editor API is ready.
	useEffect(() => {
		const shouldSync =
			!hasSyncedInitialMarkdownRef.current ||
			latestMarkdownRef.current !== editorMarkdown;
		if (!shouldSync) return;

		hasSyncedInitialMarkdownRef.current = true;
		latestMarkdownRef.current = editorMarkdown;
		const syncMarkdown = () => editorRef.current?.setMarkdown(editorMarkdown);
		syncMarkdown();
		const rafId = requestAnimationFrame(syncMarkdown);
		return () => cancelAnimationFrame(rafId);
	}, [editorMarkdown]);

	return (
		<div className={className}>
			<MDXEditor
				ref={editorRef}
				className="oqto-mdxeditor"
				contentEditableClassName="oqto-mdxeditor-content"
				markdown={editorMarkdown}
				onChange={(next, initialMarkdownNormalize) => {
					if (initialMarkdownNormalize) return;
					latestMarkdownRef.current = next;
					onChange(unescapeMdxText(next));
				}}
				plugins={editorPlugins}
			/>
		</div>
	);
}

export default MarkdownWysiwygEditor;
