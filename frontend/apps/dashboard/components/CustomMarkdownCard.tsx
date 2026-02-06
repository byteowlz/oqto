import { MarkdownRenderer } from "@/components/data-display";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { memo } from "react";

export type CustomMarkdownCardProps = {
	title: string;
	description?: string;
	content: string;
};

export const CustomMarkdownCard = memo(function CustomMarkdownCard({
	title,
	description,
	content,
}: CustomMarkdownCardProps) {
	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader>
				<CardTitle>{title}</CardTitle>
				{description && <CardDescription>{description}</CardDescription>}
			</CardHeader>
			<CardContent className="flex-1 min-h-0 overflow-auto">
				<MarkdownRenderer content={content} />
			</CardContent>
		</Card>
	);
});
