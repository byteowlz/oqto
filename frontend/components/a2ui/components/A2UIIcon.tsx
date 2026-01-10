/**
 * A2UI Icon Component
 *
 * Maps A2UI icon names to Lucide icons
 */

import type { IconComponent, IconName } from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import {
	AlertCircle,
	AlertTriangle,
	ArrowLeft,
	ArrowRight,
	Bell,
	BellOff,
	Calendar,
	CalendarDays,
	Camera,
	Check,
	CreditCard,
	Download,
	Eye,
	EyeOff,
	Folder,
	Heart,
	HeartOff,
	HelpCircle,
	Home,
	Image,
	Info,
	Lock,
	LockOpen,
	type LucideIcon,
	Mail,
	MapPin,
	Menu,
	MoreHorizontal,
	MoreVertical,
	Paperclip,
	Pencil,
	Phone,
	Plus,
	Printer,
	RefreshCw,
	Search,
	Send,
	Settings,
	Share2,
	ShoppingCart,
	Star,
	StarHalf,
	StarOff,
	Trash2,
	Upload,
	User,
	UserCircle,
	X,
} from "lucide-react";

interface A2UIIconProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
}

const iconMap: Record<IconName, LucideIcon> = {
	accountCircle: UserCircle,
	add: Plus,
	arrowBack: ArrowLeft,
	arrowForward: ArrowRight,
	attachFile: Paperclip,
	calendarToday: Calendar,
	call: Phone,
	camera: Camera,
	check: Check,
	close: X,
	delete: Trash2,
	download: Download,
	edit: Pencil,
	event: CalendarDays,
	error: AlertCircle,
	favorite: Heart,
	favoriteOff: HeartOff,
	folder: Folder,
	help: HelpCircle,
	home: Home,
	info: Info,
	locationOn: MapPin,
	lock: Lock,
	lockOpen: LockOpen,
	mail: Mail,
	menu: Menu,
	moreVert: MoreVertical,
	moreHoriz: MoreHorizontal,
	notificationsOff: BellOff,
	notifications: Bell,
	payment: CreditCard,
	person: User,
	phone: Phone,
	photo: Image,
	print: Printer,
	refresh: RefreshCw,
	search: Search,
	send: Send,
	settings: Settings,
	share: Share2,
	shoppingCart: ShoppingCart,
	star: Star,
	starHalf: StarHalf,
	starOff: StarOff,
	upload: Upload,
	visibility: Eye,
	visibilityOff: EyeOff,
	warning: AlertTriangle,
};

export function A2UIIcon({ props, dataModel }: A2UIIconProps) {
	const iconProps = props as unknown as IconComponent;
	const iconName = resolveBoundValue(
		iconProps.name,
		dataModel,
		"info",
	) as IconName;

	const IconComponent = iconMap[iconName];

	if (!IconComponent) {
		return <Info className="w-5 h-5" />;
	}

	return <IconComponent className="w-5 h-5" />;
}
