"use client";

import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { ChevronDown, ChevronUp, FileSpreadsheet } from "lucide-react";
import { useMemo, useState } from "react";

interface CSVViewerProps {
	content: string;
	filename?: string;
	hasHeader?: boolean;
	delimiter?: string;
	className?: string;
}

function parseCSV(content: string, delimiter = ","): string[][] {
	const rows: string[][] = [];
	let currentRow: string[] = [];
	let currentCell = "";
	let inQuotes = false;

	for (let i = 0; i < content.length; i++) {
		const char = content[i];
		const nextChar = content[i + 1];

		if (inQuotes) {
			if (char === '"' && nextChar === '"') {
				currentCell += '"';
				i++; // Skip next quote
			} else if (char === '"') {
				inQuotes = false;
			} else {
				currentCell += char;
			}
		} else {
			if (char === '"') {
				inQuotes = true;
			} else if (char === delimiter) {
				currentRow.push(currentCell);
				currentCell = "";
			} else if (char === "\n" || (char === "\r" && nextChar === "\n")) {
				currentRow.push(currentCell);
				if (currentRow.some((cell) => cell.trim() !== "")) {
					rows.push(currentRow);
				}
				currentRow = [];
				currentCell = "";
				if (char === "\r") i++; // Skip \n after \r
			} else if (char !== "\r") {
				currentCell += char;
			}
		}
	}

	// Handle last row
	if (currentCell || currentRow.length > 0) {
		currentRow.push(currentCell);
		if (currentRow.some((cell) => cell.trim() !== "")) {
			rows.push(currentRow);
		}
	}

	return rows;
}

export function CSVViewer({
	content,
	filename,
	hasHeader = true,
	delimiter = ",",
	className,
}: CSVViewerProps) {
	const [sortColumn, setSortColumn] = useState<number | null>(null);
	const [sortDirection, setSortDirection] = useState<"asc" | "desc">("asc");

	const data = useMemo(
		() => parseCSV(content, delimiter),
		[content, delimiter],
	);

	const headers = hasHeader && data.length > 0 ? data[0] : null;
	const rows = hasHeader && data.length > 0 ? data.slice(1) : data;

	const sortedRows = useMemo(() => {
		if (sortColumn === null) return rows;

		return [...rows].sort((a, b) => {
			const aVal = a[sortColumn] || "";
			const bVal = b[sortColumn] || "";

			// Try numeric comparison first
			const aNum = Number.parseFloat(aVal);
			const bNum = Number.parseFloat(bVal);

			if (!Number.isNaN(aNum) && !Number.isNaN(bNum)) {
				return sortDirection === "asc" ? aNum - bNum : bNum - aNum;
			}

			// Fall back to string comparison
			const comparison = aVal.localeCompare(bVal);
			return sortDirection === "asc" ? comparison : -comparison;
		});
	}, [rows, sortColumn, sortDirection]);

	const handleSort = (columnIndex: number) => {
		if (sortColumn === columnIndex) {
			setSortDirection((d) => (d === "asc" ? "desc" : "asc"));
		} else {
			setSortColumn(columnIndex);
			setSortDirection("asc");
		}
	};

	const columnCount = Math.max(...data.map((row) => row.length), 0);

	if (data.length === 0) {
		return (
			<div
				className={cn(
					"flex items-center justify-center h-full text-muted-foreground",
					className,
				)}
			>
				<p>No data to display</p>
			</div>
		);
	}

	return (
		<div className={cn("flex flex-col h-full", className)}>
			{/* Header */}
			<div className="flex items-center justify-between px-3 py-2 bg-muted border-b border-border shrink-0">
				<div className="flex items-center gap-2">
					<FileSpreadsheet className="w-4 h-4 text-muted-foreground" />
					{filename && (
						<span className="text-sm font-medium truncate max-w-[200px]">
							{filename}
						</span>
					)}
					<span className="text-xs text-muted-foreground">
						{rows.length} rows x {columnCount} columns
					</span>
				</div>
			</div>

			{/* Table */}
			<ScrollArea className="flex-1">
				<Table>
					{headers && (
						<TableHeader>
							<TableRow>
								{headers.map((header, i) => {
									const headerKey = header || `column-${i}`;
									return (
										<TableHead
											key={headerKey}
											className="cursor-pointer hover:bg-muted/50 select-none whitespace-nowrap"
											onClick={() => handleSort(i)}
										>
											<div className="flex items-center gap-1">
												<span>{header || `Column ${i + 1}`}</span>
												{sortColumn === i &&
													(sortDirection === "asc" ? (
														<ChevronUp className="w-3 h-3" />
													) : (
														<ChevronDown className="w-3 h-3" />
													))}
											</div>
										</TableHead>
									);
								})}
							</TableRow>
						</TableHeader>
					)}
					<TableBody>
						{sortedRows.map((row) => {
							const rowKey =
								row.map((cell, index) => `${index}:${cell}`).join("|") ||
								"row-empty";
							return (
								<TableRow key={rowKey}>
									{headers.map((header, i) => {
										const cellKey = `${rowKey}-${header || `col-${i}`}`;
										return (
											<TableCell key={cellKey} className="whitespace-nowrap">
												{row[i] || ""}
											</TableCell>
										);
									})}
								</TableRow>
							);
						})}
					</TableBody>
				</Table>
				<ScrollBar orientation="horizontal" />
			</ScrollArea>
		</div>
	);
}
