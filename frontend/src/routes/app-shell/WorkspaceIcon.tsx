/**
 * Renders a Lucide icon by name for shared workspace display.
 * Uses the workspace's configured color as the icon color.
 */
import {
	Brain,
	Building,
	Code,
	Database,
	FlaskConical,
	GitBranch,
	Globe,
	Hexagon,
	Layers,
	Network,
	Palette,
	Rocket,
	Shield,
	Terminal,
	Users,
	Zap,
} from "lucide-react";
import { memo } from "react";

import type { WorkspaceIconName } from "@/lib/api/shared-workspaces";

const ICON_MAP: Record<
	WorkspaceIconName,
	React.ComponentType<{ className?: string; style?: React.CSSProperties }>
> = {
	users: Users,
	rocket: Rocket,
	globe: Globe,
	code: Code,
	building: Building,
	shield: Shield,
	zap: Zap,
	layers: Layers,
	hexagon: Hexagon,
	terminal: Terminal,
	"flask-conical": FlaskConical,
	palette: Palette,
	brain: Brain,
	database: Database,
	network: Network,
	"git-branch": GitBranch,
};

export interface WorkspaceIconProps {
	icon: string;
	color: string;
	className?: string;
}

export const WorkspaceIcon = memo(function WorkspaceIcon({
	icon,
	color,
	className = "w-3.5 h-3.5",
}: WorkspaceIconProps) {
	const IconComponent = ICON_MAP[icon as WorkspaceIconName] ?? Users;
	return <IconComponent className={className} style={{ color }} />;
});
