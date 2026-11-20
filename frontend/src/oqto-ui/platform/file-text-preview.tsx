import { MarkdownRenderer } from "@/components/data-display/markdown-renderer";
import { CodeViewer } from "@/components/viewers/code-viewer";
import { getSyntaxLanguage } from "@/lib/file-types";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import "./file-text-preview.css";

interface FileTextPreviewProps {
	readonly name: string;
	readonly content: string;
}

/** Pure presentation over FileHost's text. No fetching or second Files authority. */
export function FileTextPreview({ name, content }: FileTextPreviewProps) {
	const { t } = useTranslation();
	const [source, setSource] = useState(false);
	const language = getSyntaxLanguage(name);
	const markdown = language === "markdown";
	return (
		<div className="wb-file-text">
			{markdown ? (
				<fieldset className="wb-file-text__modes">
					<legend className="sr-only">{name}</legend>
					<button
						type="button"
						aria-pressed={!source}
						onClick={() => setSource(false)}
					>
						{t("oqtoUi.files.renderedPreview")}
					</button>
					<button
						type="button"
						aria-pressed={source}
						onClick={() => setSource(true)}
					>
						{t("oqtoUi.files.sourcePreview")}
					</button>
				</fieldset>
			) : null}
			{markdown && !source ? (
				<MarkdownRenderer
					content={content}
					syntaxAppearance="theme"
					enableMermaid={false}
					className="wb-file-text__markdown"
				/>
			) : (
				<CodeViewer content={content} language={language} appearance="theme" />
			)}
		</div>
	);
}
