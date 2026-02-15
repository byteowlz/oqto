"use client";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { ScrollArea } from "@/components/ui/scroll-area";
import type {
	QuestionAnswer,
	QuestionInfo,
	QuestionRequest,
} from "@/lib/agent-client";
import { cn } from "@/lib/utils";
import { HelpCircle, MessageSquare } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

interface UserQuestionDialogProps {
	request: QuestionRequest | null;
	onReply: (requestId: string, answers: QuestionAnswer[]) => Promise<void>;
	onReject: (requestId: string) => Promise<void>;
	onDismiss: () => void;
}

// Clickable card wrapper for options
function OptionCard({
	children,
	isSelected,
	onClick,
}: {
	children: React.ReactNode;
	isSelected: boolean;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"w-full flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors text-left",
				isSelected
					? "border-primary bg-primary/5"
					: "border-border hover:border-muted-foreground/50 hover:bg-muted/30",
			)}
		>
			{children}
		</button>
	);
}

// Single question component
function QuestionItem({
	question,
	index,
	answer,
	onAnswerChange,
}: {
	question: QuestionInfo;
	index: number;
	answer: string[];
	onAnswerChange: (answer: string[]) => void;
}) {
	const [customInput, setCustomInput] = useState("");
	const isMultiple = question.multiple ?? false;

	// Handle option toggle for checkboxes
	const handleCheckboxChange = useCallback(
		(label: string, checked: boolean) => {
			if (checked) {
				onAnswerChange([...answer, label]);
			} else {
				onAnswerChange(answer.filter((a) => a !== label));
			}
		},
		[answer, onAnswerChange],
	);

	// Handle radio selection
	const handleRadioChange = useCallback(
		(label: string) => {
			onAnswerChange([label]);
		},
		[onAnswerChange],
	);

	// Handle custom "Other" input
	const handleCustomSubmit = useCallback(() => {
		if (!customInput.trim()) return;
		if (isMultiple) {
			onAnswerChange([...answer, customInput.trim()]);
		} else {
			onAnswerChange([customInput.trim()]);
		}
		setCustomInput("");
	}, [customInput, isMultiple, answer, onAnswerChange]);

	return (
		<div className="space-y-3">
			{/* Question header and text */}
			<div className="space-y-1">
				<div className="flex items-center gap-2">
					<span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
						{question.header}
					</span>
				</div>
				<p className="text-sm text-foreground">{question.question}</p>
			</div>

			{/* Options */}
			<div className="space-y-2">
				{isMultiple ? (
					// Multiple selection with checkboxes
					<div className="space-y-2">
						{question.options.map((option) => {
							const isSelected = answer.includes(option.label);
							return (
								<OptionCard
									key={option.label}
									isSelected={isSelected}
									onClick={() =>
										handleCheckboxChange(option.label, !isSelected)
									}
								>
									<Checkbox
										checked={isSelected}
										onCheckedChange={(checked) =>
											handleCheckboxChange(option.label, checked === true)
										}
										className="mt-0.5"
									/>
									<div className="flex-1 min-w-0">
										<span className="text-sm font-medium">{option.label}</span>
										{option.description && (
											<p className="text-xs text-muted-foreground mt-0.5">
												{option.description}
											</p>
										)}
									</div>
								</OptionCard>
							);
						})}
					</div>
				) : (
					// Single selection with radio buttons
					<RadioGroup
						value={answer[0] || ""}
						onValueChange={handleRadioChange}
						className="space-y-2"
					>
						{question.options.map((option) => {
							const isSelected = answer[0] === option.label;
							return (
								<OptionCard
									key={option.label}
									isSelected={isSelected}
									onClick={() => handleRadioChange(option.label)}
								>
									<RadioGroupItem value={option.label} className="mt-0.5" />
									<div className="flex-1 min-w-0">
										<span className="text-sm font-medium">{option.label}</span>
										{option.description && (
											<p className="text-xs text-muted-foreground mt-0.5">
												{option.description}
											</p>
										)}
									</div>
								</OptionCard>
							);
						})}
					</RadioGroup>
				)}

				{/* Custom "Other" input */}
				<div className="flex items-center gap-2 pt-1">
					<Input
						type="text"
						placeholder="Other (type custom answer)..."
						value={customInput}
						onChange={(e) => setCustomInput(e.target.value)}
						onKeyDown={(e) => {
							if (e.key === "Enter") {
								e.preventDefault();
								handleCustomSubmit();
							}
						}}
						className="flex-1 text-sm h-9"
					/>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={handleCustomSubmit}
						disabled={!customInput.trim()}
					>
						Add
					</Button>
				</div>

				{/* Show custom answers that were added */}
				{answer.some(
					(a) => !question.options.some((opt) => opt.label === a),
				) && (
					<div className="flex flex-wrap gap-1.5 pt-1">
						{answer
							.filter((a) => !question.options.some((opt) => opt.label === a))
							.map((customAnswer) => (
								<span
									key={`custom-${customAnswer}`}
									className="inline-flex items-center gap-1 px-2 py-1 bg-primary/10 text-primary text-xs rounded-md"
								>
									{customAnswer}
									<button
										type="button"
										onClick={() =>
											onAnswerChange(answer.filter((a) => a !== customAnswer))
										}
										className="hover:text-destructive"
									>
										x
									</button>
								</span>
							))}
					</div>
				)}
			</div>
		</div>
	);
}

export function UserQuestionDialog({
	request,
	onReply,
	onReject,
	onDismiss,
}: UserQuestionDialogProps) {
	const [isSubmitting, setIsSubmitting] = useState(false);
	const [answers, setAnswers] = useState<QuestionAnswer[]>([]);

	// Initialize answers array when request changes
	useEffect(() => {
		if (request) {
			setAnswers(request.questions.map(() => []));
		}
	}, [request]);

	const handleAnswerChange = useCallback(
		(questionIndex: number, answer: string[]) => {
			setAnswers((prev) => {
				const next = [...prev];
				next[questionIndex] = answer;
				return next;
			});
		},
		[],
	);

	const handleSubmit = useCallback(async () => {
		if (!request || isSubmitting) return;

		setIsSubmitting(true);
		try {
			await onReply(request.id, answers);
			onDismiss();
		} catch (err) {
			console.error("Failed to submit answers:", err);
		} finally {
			setIsSubmitting(false);
		}
	}, [request, answers, onReply, onDismiss, isSubmitting]);

	const handleReject = useCallback(async () => {
		if (!request || isSubmitting) return;

		setIsSubmitting(true);
		try {
			await onReject(request.id);
			onDismiss();
		} catch (err) {
			console.error("Failed to reject question:", err);
		} finally {
			setIsSubmitting(false);
		}
	}, [request, onReject, onDismiss, isSubmitting]);

	if (!request) return null;

	const hasMultipleQuestions = request.questions.length > 1;
	const allQuestionsAnswered = answers.every((a) => a.length > 0);

	return (
		<Dialog open={!!request} onOpenChange={(open) => !open && onDismiss()}>
			<DialogContent
				className="sm:max-w-lg max-h-[85vh] flex flex-col"
				showCloseButton={false}
			>
				<DialogHeader>
					<div className="flex items-center gap-3 mb-2">
						<div className="p-2 rounded-lg border border-primary/30 bg-primary/10 text-primary">
							<HelpCircle className="w-5 h-5" />
						</div>
						<div>
							<DialogTitle className="text-base">
								{hasMultipleQuestions
									? `${request.questions.length} Questions`
									: "Question"}
							</DialogTitle>
							<p className="text-xs text-muted-foreground mt-0.5">
								The agent needs your input to continue
							</p>
						</div>
					</div>
					<DialogDescription className="sr-only">
						Answer the following questions to help the agent proceed
					</DialogDescription>
				</DialogHeader>

				<ScrollArea className="flex-1 -mx-6 px-6">
					<div className="space-y-6 py-2">
						{request.questions.map((question, idx) => (
							<QuestionItem
								key={`question-${question.header}-${question.question.slice(0, 20)}`}
								question={question}
								index={idx}
								answer={answers[idx] || []}
								onAnswerChange={(answer) => handleAnswerChange(idx, answer)}
							/>
						))}
					</div>
				</ScrollArea>

				<DialogFooter className="flex-col sm:flex-row gap-2 pt-4 border-t">
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={handleReject}
						disabled={isSubmitting}
						className="sm:mr-auto"
					>
						{isSubmitting ? "..." : "Skip"}
					</Button>
					<Button
						type="button"
						size="sm"
						onClick={handleSubmit}
						disabled={isSubmitting || !allQuestionsAnswered}
					>
						{isSubmitting ? "Submitting..." : "Submit Answers"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

// Banner component for showing pending questions count
export function UserQuestionBanner({
	count,
	onClick,
}: {
	count: number;
	onClick: () => void;
}) {
	if (count === 0) return null;

	return (
		<button
			type="button"
			onClick={onClick}
			className="w-full flex items-center justify-between gap-2 px-3 py-2 bg-primary/10 border border-primary/30 text-primary hover:bg-primary/20 transition-colors"
		>
			<div className="flex items-center gap-2">
				<MessageSquare className="w-4 h-4" />
				<span className="text-sm font-medium">
					{count} question{count !== 1 ? "s" : ""} pending
				</span>
			</div>
			<span className="text-xs">Click to answer</span>
		</button>
	);
}
