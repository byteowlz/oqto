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
import { AlertCircle, Check, Loader2, RotateCcw, Save } from "lucide-react";
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
		<div className="space-y-0 sm:space-y-4">
			{/* Error message */}
			{error && (
				<div className="flex items-center gap-2 px-4 sm:px-0 py-2 bg-destructive/10 text-destructive sm:rounded-md">
					<AlertCircle className="h-4 w-4 flex-shrink-0" />
					<span className="text-sm">{error}</span>
				</div>
			)}

			{/* Floating action buttons */}
			{(hasChanges || isAdmin) && (
				<div className="fixed bottom-6 right-6 z-20 flex items-center gap-2">
					{isAdmin && (
						<Button
							type="button"
							variant="outline"
							size="icon"
							onClick={handleReload}
							className="h-10 w-10 rounded-full shadow-lg bg-background"
						>
							<RotateCcw className="h-4 w-4" />
						</Button>
					)}
					{hasChanges && (
						<Button
							type="button"
							size="sm"
							onClick={handleSave}
							disabled={saving}
							className="h-10 px-4 rounded-full shadow-lg"
						>
							{saving ? (
								<Loader2 className="h-4 w-4 animate-spin" />
							) : success ? (
								<Check className="h-4 w-4" />
							) : (
								<>
									<Save className="h-4 w-4 mr-2" />
									Save
								</>
							)}
						</Button>
					)}
				</div>
			)}

			{/* Settings sections */}
			<div className="space-y-2 sm:space-y-6">
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
}

function SettingsSection({
	category,
	properties,
	values,
	getEffectiveValue,
	isModified,
	onValueChange,
	onReset,
}: SettingsSectionProps) {
	return (
		<div>
			{/* Section header */}
			<div className="px-4 sm:px-0 pt-4 pb-2">
				<h3 className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
					{category}
				</h3>
			</div>
			{/* Section content */}
			<div className="sm:bg-background/50 sm:border sm:border-border sm:rounded-lg overflow-hidden">
				<div className="divide-y divide-border/50">
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
			<div className="px-4 sm:px-3 py-3">
				<div className="mb-2">
					<Label className="font-medium text-sm">{label}</Label>
					{property.description && (
						<p className="text-xs text-muted-foreground mt-0.5">
							{property.description}
						</p>
					)}
				</div>
				<div className="space-y-0 divide-y divide-border/30 bg-muted/30 rounded-md overflow-hidden">
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
			<div className="flex items-center justify-between gap-3 px-4 sm:px-3 py-3 sm:bg-background/30 sm:border sm:border-border/50 sm:rounded-md">
				<div className="flex-1 min-w-0">
					<div className="flex items-center gap-2">
						<Label htmlFor={fullPath} className="text-sm font-normal">
							{label}
						</Label>
						{modified && (
							<span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
						)}
					</div>
					{property.description && (
						<p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
							{property.description}
						</p>
					)}
				</div>
				<div className="flex items-center gap-1 flex-shrink-0">
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
							className="h-8 w-8 p-0 opacity-50 hover:opacity-100"
						>
							<RotateCcw className="h-3.5 w-3.5" />
						</Button>
					)}
				</div>
			</div>
		);
	}

	return (
		<div className="px-4 sm:px-3 py-3 sm:bg-background/30 sm:border sm:border-border/50 sm:rounded-md space-y-2">
			<div className="flex items-center justify-between gap-2">
				<div className="flex items-center gap-2 min-w-0">
					<Label htmlFor={fullPath} className="text-sm font-normal truncate">
						{label}
					</Label>
					{modified && (
						<span className="w-1.5 h-1.5 rounded-full bg-amber-500 flex-shrink-0" />
					)}
					{isConfigured && !modified && (
						<Badge
							variant="secondary"
							className="text-[10px] px-1.5 py-0 flex-shrink-0"
						>
							set
						</Badge>
					)}
				</div>
				{property["x-sensitive"] && (
					<Badge
						variant="outline"
						className="text-[10px] px-1.5 py-0 flex-shrink-0"
					>
						secret
					</Badge>
				)}
			</div>
			{property.description && (
				<p className="text-xs text-muted-foreground line-clamp-2">
					{property.description}
				</p>
			)}
			<div className="flex items-center gap-2">
				{property.enum ? (
					<Select
						value={String(value ?? "")}
						onValueChange={(v) => onValueChange(fullPath, v)}
					>
						<SelectTrigger
							className={cn(
								"w-full h-10 text-sm bg-background",
								modified && "ring-1 ring-amber-500",
							)}
						>
							<SelectValue placeholder="Select..." />
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
						placeholder={hasDefault ? `${setting?.default}` : undefined}
						onChange={(e) => {
							const v =
								type === "integer"
									? Number.parseInt(e.target.value)
									: Number.parseFloat(e.target.value);
							if (!Number.isNaN(v)) onValueChange(fullPath, v);
						}}
						className={cn(
							"h-10 text-sm bg-background",
							modified && "ring-1 ring-amber-500",
						)}
					/>
				) : (
					<Input
						id={fullPath}
						type={property["x-sensitive"] ? "password" : "text"}
						value={
							value === null || value === undefined
								? ""
								: typeof value === "object"
									? JSON.stringify(value)
									: String(value)
						}
						placeholder={hasDefault ? `${setting?.default}` : undefined}
						onChange={(e) => {
							const text = e.target.value;
							// Try to parse as JSON if it looks like an object/array
							if (
								(text.startsWith("{") && text.endsWith("}")) ||
								(text.startsWith("[") && text.endsWith("]"))
							) {
								try {
									onValueChange(fullPath, JSON.parse(text));
									return;
								} catch {
									// Fall through to string value
								}
							}
							onValueChange(fullPath, text);
						}}
						className={cn(
							"h-10 text-sm bg-background",
							modified && "ring-1 ring-amber-500",
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
						className="h-10 w-10 p-0 flex-shrink-0 opacity-50 hover:opacity-100"
					>
						<RotateCcw className="h-4 w-4" />
					</Button>
				)}
			</div>
		</div>
	);
}
