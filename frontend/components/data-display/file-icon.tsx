type IconType =
	| "folder"
	| "file"
	| "pdf"
	| "word"
	| "ppt"
	| "txt"
	| "md"
	| "json"
	| "code"
	| "image"
	| "terminal";

// Map file extensions to icon types
function getIconType(filename: string, isDirectory: boolean): IconType {
	if (isDirectory) return "folder";

	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();

	switch (ext) {
		case ".pdf":
			return "pdf";
		case ".doc":
		case ".docx":
			return "word";
		case ".ppt":
		case ".pptx":
			return "ppt";
		case ".txt":
			return "txt";
		case ".md":
		case ".mdx":
		case ".markdown":
			return "md";
		case ".json":
		case ".jsonc":
			return "json";
		case ".js":
		case ".jsx":
		case ".ts":
		case ".tsx":
		case ".py":
		case ".rb":
		case ".go":
		case ".rs":
		case ".java":
		case ".c":
		case ".cpp":
		case ".h":
		case ".hpp":
		case ".cs":
		case ".php":
		case ".swift":
		case ".kt":
		case ".scala":
		case ".html":
		case ".htm":
		case ".css":
		case ".scss":
		case ".sass":
		case ".less":
		case ".xml":
		case ".yaml":
		case ".yml":
		case ".toml":
		case ".ini":
		case ".cfg":
		case ".conf":
		case ".sh":
		case ".bash":
		case ".zsh":
		case ".fish":
		case ".sql":
		case ".graphql":
		case ".vue":
		case ".svelte":
			return "code";
		case ".png":
		case ".jpg":
		case ".jpeg":
		case ".gif":
		case ".svg":
		case ".webp":
		case ".ico":
		case ".bmp":
		case ".tiff":
			return "image";
		default:
			return "file";
	}
}

// Map icon types to file names (without color suffix)
const iconFileMap: Record<IconType, string> = {
	folder: "FOLDER",
	file: "FILE",
	pdf: "PDF",
	word: "Word",
	ppt: "PPT_white", // This one has different naming
	txt: "TXT",
	md: "MD",
	json: "JSON",
	code: "CODE",
	image: "IMAGE",
	terminal: "Terminal",
};

interface FileIconProps {
	filename: string;
	isDirectory?: boolean;
	size?: number;
	className?: string;
}

export function FileIcon({
	filename,
	isDirectory = false,
	size = 24,
	className,
}: FileIconProps) {
	const iconType = getIconType(filename, isDirectory);
	const iconBase = iconFileMap[iconType];

	// Use CSS to show different icons based on theme
	return (
		<span
			className={`inline-flex items-center justify-center ${className ?? ""}`}
			style={{ width: size, height: size }}
		>
			{/* Light mode icon (black) - hidden in dark mode */}
			<img
				src={`/icons/${iconBase}_black.svg`}
				alt={iconType}
				width={size}
				height={size}
				className="dark:hidden"
			/>
			{/* Dark mode icon (white) - hidden in light mode */}
			<img
				src={`/icons/${iconBase}_white.svg`}
				alt={iconType}
				width={size}
				height={size}
				className="hidden dark:block"
			/>
		</span>
	);
}
