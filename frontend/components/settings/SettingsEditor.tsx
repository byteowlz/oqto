"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
	type SettingsValues,
	getSettingsSchema,
	getSettingsValues,
	reloadSettings,
	updateSettingsValues,
} from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import {
	AlertCircle,
	Check,
	ChevronDown,
	ChevronRight,
	Loader2,
	RotateCcw,
	Save,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

interface SettingsEditorProps {
	/** App to edit settings for (e.g., "octo", "mmry") */
	app: string;
	/** Title to display */
	title?: string;
	/** Whether user is admin */
	isAdmin?: boolean;
}

interface SchemaProperty {
	type?: string | string[];
	description?: string;
	default?: unknown;
	enum?: string[];
	minimum?: number;
	maximum?: number;
	minLength?: number;
	maxLength?: number;
	properties?: Record<string, SchemaProperty>;
	"x-scope"?: string;
	"x-category"?: string;
	"x-sensitive"?: boolean;
}

interface Schema {
	properties?: Record<string, SchemaProperty>;
	title?: string;
	description?: string;
}

export function SettingsEditor({
	app,
	title,
	isAdmin = false,
}: SettingsEditorProps) {
	const [schema, setSchema] = useState<Schema | null>(null);
	const [values, setValues] = useState<SettingsValues>({});
	const [pendingChanges, setPendingChanges] = useState<Record<string, unknown>>(
		{},
	);
	const [loading, setLoading] = useState(true);
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [success, setSuccess] = useState(false);
	const [expandedSections, setExpandedSections] = useState<Set<string>>(
		new Set(),
	);

	// Load schema and values
	const loadSettings = useCallback(async () => {
		setLoading(true);
		setError(null);
		try {
			const [schemaData, valuesData] = await Promise.all([
				getSettingsSchema(app),
				getSettingsValues(app),
			]);
			setSchema(schemaData as Schema);
			setValues(valuesData);
			setPendingChanges({});
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to load settings");
		} finally {
			setLoading(false);
		}
	}, [app]);

	useEffect(() => {
		loadSettings();
	}, [loadSettings]);

	// Save changes
	const handleSave = useCallback(async () => {
		if (Object.keys(pendingChanges).length === 0) return;

		setSaving(true);
		setError(null);
		setSuccess(false);
		try {
			const newValues = await updateSettingsValues(app, {
				values: pendingChanges,
			});
			setValues(newValues);
			setPendingChanges({});
			setSuccess(true);
			setTimeout(() => setSuccess(false), 2000);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to save settings");
		} finally {
			setSaving(false);
		}
	}, [app, pendingChanges]);

	// Reload from disk (admin only)
	const handleReload = useCallback(async () => {
		setError(null);
		try {
			await reloadSettings(app);
			await loadSettings();
		} catch (err) {
			setError(
				err instanceof Error ? err.message : "Failed to reload settings",
			);
		}
	}, [app, loadSettings]);

	// Update a value
	const handleValueChange = useCallback((path: string, value: unknown) => {
		setPendingChanges((prev) => ({ ...prev, [path]: value }));
	}, []);

	// Reset a value to default
	const handleReset = useCallback(
		(path: string) => {
			const setting = values[path];
			if (setting?.default !== undefined) {
				setPendingChanges((prev) => ({ ...prev, [path]: setting.default }));
			}
		},
		[values],
	);

	// Get effective value (pending change or current)
	const getEffectiveValue = useCallback(
		(path: string): unknown => {
			if (path in pendingChanges) return pendingChanges[path];
			return values[path]?.value;
		},
		[pendingChanges, values],
	);

	// Check if value has been modified
	const isModified = useCallback(
		(path: string): boolean => {
			return path in pendingChanges;
		},
		[pendingChanges],
	);

	// Toggle section expansion
	const toggleSection = useCallback((section: string) => {
		setExpandedSections((prev) => {
			const next = new Set(prev);
			if (next.has(section)) {
				next.delete(section);
			} else {
				next.add(section);
			}
			return next;
		});
	}, []);

	if (loading) {
		return (
			<div className="flex items-center justify-center p-8">
				<Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
			</div>
		);
	}

	if (!schema) {
		return (
			<div className="p-4 text-center text-muted-foreground">
				No settings available for {app}
			</div>
		);
	}

	// Group properties by x-category
	const groupedProperties = groupByCategory(schema.properties || {});
	const hasChanges = Object.keys(pendingChanges).length > 0;

	return (
		<div className="space-y-4 sm:space-y-6">
			{/* Header */}
			<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
				<div className="min-w-0">
					<h2 className="text-base sm:text-lg font-semibold truncate">
						{title || schema.title || `${app} Settings`}
					</h2>
					{schema.description && (
						<p className="text-xs sm:text-sm text-muted-foreground line-clamp-2">
							{schema.description}
						</p>
					)}
				</div>
				<div className="flex items-center gap-2 flex-shrink-0">
					{isAdmin && (
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={handleReload}
							className="h-8 px-2 sm:px-3"
						>
							<RotateCcw className="h-4 w-4 sm:mr-1" />
							<span className="hidden sm:inline">Reload</span>
						</Button>
					)}
					<Button
						type="button"
						size="sm"
						onClick={handleSave}
						disabled={!hasChanges || saving}
						className="h-8 px-2 sm:px-3"
					>
						{saving ? (
							<Loader2 className="h-4 w-4 sm:mr-1 animate-spin" />
						) : success ? (
							<Check className="h-4 w-4 sm:mr-1" />
						) : (
							<Save className="h-4 w-4 sm:mr-1" />
						)}
						<span className="hidden sm:inline">
							{saving ? "Saving..." : success ? "Saved" : "Save Changes"}
						</span>
						<span className="sm:hidden">
							{saving ? "..." : success ? "OK" : "Save"}
						</span>
					</Button>
				</div>
			</div>

			{/* Error message */}
			{error && (
				<div className="flex items-center gap-2 p-3 bg-destructive/10 text-destructive rounded-md">
					<AlertCircle className="h-4 w-4" />
					<span className="text-sm">{error}</span>
				</div>
			)}

			{/* Settings sections */}
			<div className="space-y-3 sm:space-y-4">
				{Object.entries(groupedProperties).map(([category, properties]) => (
					<SettingsSection
						key={category}
						category={category}
						properties={properties}
						values={values}
						getEffectiveValue={getEffectiveValue}
						isModified={isModified}
						onValueChange={handleValueChange}
						onReset={handleReset}
						expanded={expandedSections.has(category)}
						onToggle={() => toggleSection(category)}
					/>
				))}
			</div>
		</div>
	);
}

// Group properties by x-category
function groupByCategory(
	properties: Record<string, SchemaProperty>,
): Record<string, Record<string, SchemaProperty>> {
	const groups: Record<string, Record<string, SchemaProperty>> = {};

	for (const [key, prop] of Object.entries(properties)) {
		const category = prop["x-category"] || "General";
		if (!groups[category]) groups[category] = {};
		groups[category][key] = prop;
	}

	return groups;
}

interface SettingsSectionProps {
	category: string;
	properties: Record<string, SchemaProperty>;
	values: SettingsValues;
	getEffectiveValue: (path: string) => unknown;
	isModified: (path: string) => boolean;
	onValueChange: (path: string, value: unknown) => void;
	onReset: (path: string) => void;
	expanded: boolean;
	onToggle: () => void;
}

function SettingsSection({
	category,
	properties,
	values,
	getEffectiveValue,
	isModified,
	onValueChange,
	onReset,
	expanded,
	onToggle,
}: SettingsSectionProps) {
	const propertyCount = Object.keys(properties).length;

	return (
		<div className="bg-background/50 border border-border rounded-lg overflow-hidden">
			<button
				type="button"
				onClick={onToggle}
				className="w-full flex items-center justify-between p-3 sm:p-4 hover:bg-muted/50 transition-colors"
			>
				<div className="flex items-center gap-2">
					<span className="font-medium text-sm sm:text-base">{category}</span>
					<span className="text-xs text-muted-foreground">
						({propertyCount} {propertyCount === 1 ? "setting" : "settings"})
					</span>
				</div>
				{expanded ? (
					<ChevronDown className="h-4 w-4 text-muted-foreground flex-shrink-0" />
				) : (
					<ChevronRight className="h-4 w-4 text-muted-foreground flex-shrink-0" />
				)}
			</button>
			{expanded && (
				<div className="border-t border-border bg-muted/20 p-3 sm:p-4">
					<div className="space-y-4 sm:space-y-5">
						{Object.entries(properties).map(([key, prop]) => (
							<SettingsField
								key={key}
								path={key}
								property={prop}
								values={values}
								getEffectiveValue={getEffectiveValue}
								isModified={isModified}
								onValueChange={onValueChange}
								onReset={onReset}
							/>
						))}
					</div>
				</div>
			)}
		</div>
	);
}

interface SettingsFieldProps {
	path: string;
	property: SchemaProperty;
	values: SettingsValues;
	getEffectiveValue: (path: string) => unknown;
	isModified: (path: string) => boolean;
	onValueChange: (path: string, value: unknown) => void;
	onReset: (path: string) => void;
	prefix?: string;
}

// Format a path segment into a human-readable label
function formatLabel(path: string): string {
	return path
		.split("_")
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
		.join(" ");
}

// Format enum value into human-readable label
function formatEnumLabel(value: string): string {
	// Handle snake_case and kebab-case
	return value
		.split(/[_-]/)
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
		.join(" ");
}

function SettingsField({
	path,
	property,
	values,
	getEffectiveValue,
	isModified,
	onValueChange,
	onReset,
	prefix = "",
}: SettingsFieldProps) {
	const fullPath = prefix ? `${prefix}.${path}` : path;
	const value = getEffectiveValue(fullPath);
	const setting = values[fullPath];
	const modified = isModified(fullPath);
	const isConfigured = setting?.is_configured || modified;
	const hasDefault = setting?.default !== undefined;
	const label = formatLabel(path);

	// Handle nested objects
	if (property.type === "object" && property.properties) {
		return (
			<div className="space-y-4 p-3 bg-background/30 border border-border/50 rounded-md">
				<div>
					<Label className="font-medium text-sm">{label}</Label>
					{property.description && (
						<p className="text-xs text-muted-foreground mt-0.5">
							{property.description}
						</p>
					)}
				</div>
				<div className="space-y-4 pl-3 border-l-2 border-primary/30">
					{Object.entries(property.properties).map(([key, nestedProp]) => (
						<SettingsField
							key={key}
							path={key}
							property={nestedProp}
							values={values}
							getEffectiveValue={getEffectiveValue}
							isModified={isModified}
							onValueChange={onValueChange}
							onReset={onReset}
							prefix={fullPath}
						/>
					))}
				</div>
			</div>
		);
	}

	const type = Array.isArray(property.type) ? property.type[0] : property.type;

	// Boolean fields get a special compact layout
	if (type === "boolean") {
		return (
			<div className="flex items-start justify-between gap-4 p-3 bg-background/30 border border-border/50 rounded-md">
				<div className="flex-1 min-w-0">
					<div className="flex flex-wrap items-center gap-1.5">
						<Label htmlFor={fullPath} className="text-sm font-medium">
							{label}
						</Label>
						{modified && (
							<Badge
								variant="default"
								className="text-[10px] px-1.5 py-0 bg-amber-500"
							>
								modified
							</Badge>
						)}
					</div>
					{property.description && (
						<p className="text-xs text-muted-foreground mt-0.5">
							{property.description}
						</p>
					)}
				</div>
				<div className="flex items-center gap-2 flex-shrink-0">
					<Switch
						id={fullPath}
						checked={Boolean(value)}
						onCheckedChange={(checked) => onValueChange(fullPath, checked)}
					/>
					{hasDefault && isConfigured && (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => onReset(fullPath)}
							title="Reset to default"
							className="h-7 w-7 p-0"
						>
							<RotateCcw className="h-3 w-3" />
						</Button>
					)}
				</div>
			</div>
		);
	}

	return (
		<div className="space-y-2 p-3 bg-background/30 border border-border/50 rounded-md">
			<div className="flex flex-wrap items-center gap-1.5 sm:gap-2">
				<Label htmlFor={fullPath} className="text-sm font-medium">
					{label}
				</Label>
				{isConfigured && !modified && (
					<Badge
						variant="secondary"
						className="text-[10px] sm:text-xs px-1.5 py-0"
					>
						configured
					</Badge>
				)}
				{modified && (
					<Badge
						variant="default"
						className="text-[10px] sm:text-xs px-1.5 py-0 bg-amber-500"
					>
						modified
					</Badge>
				)}
				{property["x-sensitive"] && (
					<Badge
						variant="outline"
						className="text-[10px] sm:text-xs px-1.5 py-0"
					>
						sensitive
					</Badge>
				)}
			</div>
			{property.description && (
				<p className="text-xs text-muted-foreground">{property.description}</p>
			)}
			<div className="flex items-center gap-2">
				{property.enum ? (
					<Select
						value={String(value ?? "")}
						onValueChange={(v) => onValueChange(fullPath, v)}
					>
						<SelectTrigger
							className={cn(
								"w-full h-9 text-sm bg-background",
								modified && "border-amber-500",
							)}
						>
							<SelectValue placeholder="Select an option..." />
						</SelectTrigger>
						<SelectContent>
							{property.enum.map((option) => (
								<SelectItem key={option} value={option}>
									{formatEnumLabel(option)}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				) : type === "integer" || type === "number" ? (
					<Input
						id={fullPath}
						type="number"
						value={value !== undefined ? String(value) : ""}
						min={property.minimum}
						max={property.maximum}
						placeholder={
							hasDefault ? `Default: ${setting?.default}` : undefined
						}
						onChange={(e) => {
							const v =
								type === "integer"
									? Number.parseInt(e.target.value)
									: Number.parseFloat(e.target.value);
							if (!Number.isNaN(v)) onValueChange(fullPath, v);
						}}
						className={cn(
							"h-9 text-sm bg-background",
							modified && "border-amber-500",
						)}
					/>
				) : (
					<Input
						id={fullPath}
						type={property["x-sensitive"] ? "password" : "text"}
						value={String(value ?? "")}
						placeholder={
							hasDefault ? `Default: ${setting?.default}` : undefined
						}
						onChange={(e) => onValueChange(fullPath, e.target.value)}
						className={cn(
							"h-9 text-sm bg-background",
							modified && "border-amber-500",
						)}
					/>
				)}
				{hasDefault && isConfigured && (
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={() => onReset(fullPath)}
						title="Reset to default"
						className="h-9 w-9 p-0 flex-shrink-0"
					>
						<RotateCcw className="h-3 w-3" />
					</Button>
				)}
			</div>
			{hasDefault && !modified && (
				<p className="text-[11px] text-muted-foreground/70">
					Default: {JSON.stringify(setting?.default)}
				</p>
			)}
		</div>
	);
}
