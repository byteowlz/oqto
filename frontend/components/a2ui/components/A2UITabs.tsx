/**
 * A2UI Tabs Component
 */

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type {
	A2UIComponentInstance,
	A2UISurfaceState,
	TabsComponent,
} from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { ComponentRenderer } from "../A2UIRenderer";

interface A2UITabsProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	surface: A2UISurfaceState;
	onAction: (actionName: string, context: Record<string, unknown>) => void;
	onDataChange?: (path: string, value: unknown) => void;
	resolveChildren: (childIds: string[]) => A2UIComponentInstance[];
}

export function A2UITabs({
	props,
	dataModel,
	surface,
	onAction,
	onDataChange,
}: A2UITabsProps) {
	const tabsProps = props as unknown as TabsComponent;
	const tabItems = tabsProps.tabItems || [];

	if (tabItems.length === 0) {
		return null;
	}

	const defaultTab = "tab-0";

	return (
		<Tabs defaultValue={defaultTab} className="w-full">
			<TabsList className="w-full justify-start">
				{tabItems.map((item, index) => {
					const title = resolveBoundValue(
						item.title,
						dataModel,
						`Tab ${index + 1}`,
					);
					const tabKey = item.child || `tab-${index}`;
					return (
						<TabsTrigger key={tabKey} value={`tab-${index}`}>
							{title}
						</TabsTrigger>
					);
				})}
			</TabsList>
			{tabItems.map((item, index) => {
				const childComponent = surface.components.get(item.child);
				const tabKey = item.child || `content-${index}`;
				return (
					<TabsContent key={tabKey} value={`tab-${index}`}>
						{childComponent ? (
							<ComponentRenderer
								instance={childComponent}
								surface={surface}
								onAction={(a) => onAction(a.name, a.context)}
								onDataChange={onDataChange}
							/>
						) : (
							<div className="text-muted-foreground text-sm p-2">
								Missing content: {item.child}
							</div>
						)}
					</TabsContent>
				);
			})}
		</Tabs>
	);
}
