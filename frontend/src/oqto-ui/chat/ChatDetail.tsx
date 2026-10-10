/**
 * How much of the Agent's working the Chat shows. The shared renderer reads
 * one host-wide setting, so this writes to that same store rather than
 * keeping a second one: choosing a level here changes every Chat at once.
 *
 * Levels are named for what a reader sees, not for a number: what the Agent
 * said, a summary of what it did, or every tool call and thought.
 */

import { type ChatVerbosity, useChatVerbosity } from "@/lib/chat-verbosity";
import { useId } from "react";
import { useTranslation } from "react-i18next";

const LEVELS: readonly ChatVerbosity[] = [1, 2, 3];

const KEYS: { readonly [level in ChatVerbosity]: string } = {
	1: "answers",
	2: "summary",
	3: "everything",
};

export function ChatDetail() {
	const { t } = useTranslation();
	const { verbosity, setVerbosity } = useChatVerbosity();
	const headingId = useId();
	// Native radios: one tab stop, arrow keys move the choice, checked state
	// is announced without hand-maintained ARIA.
	return (
		<section className="wb-settings-pane__section">
			<h2 id={headingId}>{t("oqtoUi.chatDetail.label")}</h2>
			<fieldset className="wb-chat-detail" aria-labelledby={headingId}>
				{LEVELS.map((level) => (
					<label key={level} data-active={verbosity === level || undefined}>
						<input
							type="radio"
							name={headingId}
							value={level}
							checked={verbosity === level}
							onChange={() => setVerbosity(level)}
						/>
						<strong>{t(`oqtoUi.chatDetail.${KEYS[level]}`)}</strong>
						<small>{t(`oqtoUi.chatDetail.${KEYS[level]}Hint`)}</small>
					</label>
				))}
			</fieldset>
		</section>
	);
}
