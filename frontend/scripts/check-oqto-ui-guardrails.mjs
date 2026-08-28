import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";
import ts from "typescript";

const DEFAULT_SOURCE_ROOT = path.join(process.cwd(), "src", "oqto-ui");
const DEFAULT_EXCEPTIONS_PATH = path.join(
	process.cwd(),
	"scripts",
	"oqto-ui-guardrail-exceptions.json",
);
const SOURCE_EXTENSIONS = new Set([".ts", ".tsx"]);
const LAYERS = [
	"app",
	"layout",
	"sessions",
	"chat",
	"engine",
	"files",
	"gallery",
	"theme",
	"platform",
	"dev",
];
const ALLOWED_DIRECT_IMPORTS = {
	engine: new Set(["engine"]),
	app: new Set(LAYERS),
	layout: new Set(["layout", "platform"]),
	sessions: new Set(["sessions", "platform"]),
	chat: new Set(["chat", "engine", "platform"]),
	files: new Set(["files", "platform"]),
	gallery: new Set(["gallery", "platform"]),
	theme: new Set(["theme", "platform"]),
	platform: new Set(["engine", "platform"]),
	dev: new Set(["dev", "engine", "platform"]),
};

const ALLOWED_PROJECT_ALIASES = {
	app: ["@/hooks/use-document-event"],
	layout: [],
	sessions: [],
	chat: [
		"@/components/chat/tool-call-card",
		"@/components/data-display/markdown-renderer",
		"@/components/viewers/resource-preview-host",
		"@/hooks/use-document-event",
		"@/hooks/use-mobile",
		"@/hooks/use-mount-effect",
		"@/lib/file-types",
		"@/lib/message-part",
		"@/lib/workspace-resource",
	],
	files: [],
	gallery: [],
	theme: [],
	platform: ["@/src/generated"],
	dev: [],
};

const FORBIDDEN_LEGACY_IMPORTS = [
	"@/hooks/use-app",
	"@/components/contexts",
	"@/src/routes/AppShellRoute",
	"@/src/routes/app-shell",
	"@/features/sessions/SessionScreen",
	"@/features/chat/components/ChatView",
	"@/lib/ws-manager",
	"@/lib/ws-client",
	"@/lib/app-registry",
	"@/features/chat",
	"@/features/sessions",
];

// Scripted fixture adapters live in dev/ but are platform adapters by
// nature; only their *-server/platform files may touch host APIs.
const SCRIPTED_ADAPTER_PATTERN = /scripted-(chat-server|platform)\.ts$/;

const ADAPTER_ONLY_GLOBALS = new Set([
	"fetch",
	"localStorage",
	"sessionStorage",
	"setTimeout",
	"clearTimeout",
	"setInterval",
	"clearInterval",
	"WebSocket",
	"EventSource",
	"MutationObserver",
	"ResizeObserver",
	"IntersectionObserver",
]);

const USER_FACING_JSX_ATTRIBUTES = new Set([
	"aria-label",
	"aria-description",
	"alt",
	"label",
	"placeholder",
	"title",
]);

const USER_FACING_OBJECT_PROPERTIES = new Set([
	"description",
	"emptyMessage",
	"label",
	"message",
	"subtitle",
	"title",
]);

const VISUAL_VALUE_PATTERNS = [
	{ name: "hex color", pattern: /#[0-9a-fA-F]{3,8}\b/ },
	{
		name: "literal color function",
		pattern: /\b(?:rgb|hsl|oklch|lab|lch)\s*\(/i,
	},
	{
		name: "arbitrary Tailwind value",
		pattern: /\b[a-z][a-z0-9-]*-\[[^\]]+\]/i,
	},
	{
		name: "feature-owned shadow utility",
		pattern: /(?:^|\s)(?:drop-)?shadow(?:-|\s|$)/,
	},
	{
		name: "feature-owned radius utility",
		pattern: /(?:^|\s)rounded(?:-|\s|$)/,
	},
];

function parseArgs(argv) {
	let sourceRoot = DEFAULT_SOURCE_ROOT;
	let exceptionsPath = DEFAULT_EXCEPTIONS_PATH;
	let json = false;
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--source-root") {
			const value = argv[index + 1];
			if (!value) throw new Error("--source-root requires a path");
			sourceRoot = path.resolve(value);
			index += 1;
		} else if (arg === "--exceptions") {
			const value = argv[index + 1];
			if (!value) throw new Error("--exceptions requires a path");
			exceptionsPath = path.resolve(value);
			index += 1;
		} else if (arg === "--json") {
			json = true;
		} else {
			throw new Error(`Unknown argument: ${arg}`);
		}
	}
	return { sourceRoot, exceptionsPath, json };
}

async function walk(directory) {
	let entries;
	try {
		entries = await fs.readdir(directory, { withFileTypes: true });
	} catch (error) {
		if (error && typeof error === "object" && error.code === "ENOENT")
			return [];
		throw error;
	}

	const files = [];
	for (const entry of entries) {
		const absolute = path.join(directory, entry.name);
		if (entry.isDirectory()) {
			files.push(...(await walk(absolute)));
		} else if (SOURCE_EXTENSIONS.has(path.extname(entry.name))) {
			files.push(absolute);
		}
	}
	return files.sort();
}

function normalizedRelative(root, file) {
	return path.relative(root, file).replaceAll(path.sep, "/");
}

function sourceLayer(root, file) {
	const [layer] = normalizedRelative(root, file).split("/");
	return LAYERS.includes(layer) ? layer : null;
}

function moduleArea(root, file) {
	return sourceLayer(root, file);
}

function violation(rule, file, node, sourceFile, message) {
	const start = node
		? sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile))
		: { line: 0, character: 0 };
	return {
		rule,
		file,
		line: start.line + 1,
		column: start.character + 1,
		message,
	};
}

/**
 * True when every property of the style object literal is a CSS custom
 * property (name starts with "--") with a computed value. Such styles carry
 * measured layout data into the stylesheet; visual declarations stay in CSS.
 */
function jsxStyleUsesOnlyCustomProperties(styleAttribute) {
	let initializer = styleAttribute.initializer;
	// Unwrap JSX expression containers, `as ...` assertions, and parentheses.
	while (
		initializer &&
		(ts.isJsxExpression(initializer) ||
			ts.isAsExpression(initializer) ||
			ts.isParenthesizedExpression(initializer))
	) {
		initializer = initializer.expression;
	}
	if (!initializer || !ts.isObjectLiteralExpression(initializer)) {
		return false;
	}
	return initializer.properties.every((property) => {
		if (!ts.isPropertyAssignment(property)) return false;
		const name = property.name;
		return (
			(ts.isStringLiteral(name) || ts.isIdentifier(name)) &&
			name.text.startsWith("--") &&
			!ts.isStringLiteral(property.initializer)
		);
	});
}

function resolveInternalImport(
	sourceRoot,
	sourceFilePath,
	specifier,
	allFiles,
) {
	let base;
	if (specifier.startsWith(".")) {
		base = path.resolve(path.dirname(sourceFilePath), specifier);
	} else if (specifier.startsWith("@/src/oqto-ui/")) {
		base = path.join(sourceRoot, specifier.slice("@/src/oqto-ui/".length));
	} else {
		return null;
	}

	const candidates = [
		base,
		`${base}.ts`,
		`${base}.tsx`,
		path.join(base, "index.ts"),
		path.join(base, "index.tsx"),
	];
	return candidates.find((candidate) => allFiles.has(candidate)) ?? null;
}

function isInside(root, candidate) {
	const relative = path.relative(root, candidate);
	return (
		relative === "" ||
		(!relative.startsWith("..") && !path.isAbsolute(relative))
	);
}

function projectAliasAllowed(layer, specifier) {
	if (!layer) return false;
	return ALLOWED_PROJECT_ALIASES[layer].some(
		(prefix) => specifier === prefix || specifier.startsWith(`${prefix}/`),
	);
}

function isLegacyImport(specifier) {
	return FORBIDDEN_LEGACY_IMPORTS.some(
		(prefix) => specifier === prefix || specifier.startsWith(`${prefix}/`),
	);
}

async function readExceptions(exceptionsPath) {
	let raw;
	try {
		raw = await fs.readFile(exceptionsPath, "utf8");
	} catch (error) {
		if (error && typeof error === "object" && error.code === "ENOENT") {
			throw new Error(
				`OqtoUI exception manifest is missing: ${exceptionsPath}`,
			);
		}
		throw error;
	}
	const parsed = JSON.parse(raw);
	if (parsed?.version !== 1 || !Array.isArray(parsed.exceptions)) {
		throw new Error("Invalid OqtoUI exception manifest schema");
	}
	for (const [index, exception] of parsed.exceptions.entries()) {
		if (
			exception?.approvedBy !== "owner" ||
			typeof exception.rule !== "string" ||
			typeof exception.file !== "string" ||
			typeof exception.reason !== "string" ||
			exception.reason.trim().length < 10 ||
			typeof exception.removalCondition !== "string" ||
			exception.removalCondition.trim().length < 10 ||
			typeof exception.issue !== "string" ||
			!/^oqto-[a-z0-9.]+$/.test(exception.issue)
		) {
			throw new Error(
				`Invalid OqtoUI exception at index ${index}; owner approval (approvedBy: "owner"), exact rule/file, reason, removalCondition, and oqto issue are required`,
			);
		}
		if (exception.line !== undefined && !Number.isInteger(exception.line)) {
			throw new Error(`Invalid OqtoUI exception line at index ${index}`);
		}
	}
	return parsed.exceptions;
}

function applyExceptions(violations, exceptions) {
	const used = new Set();
	const remaining = violations.filter((item) => {
		const index = exceptions.findIndex(
			(exception) =>
				exception.rule === item.rule &&
				exception.file === item.file &&
				(exception.line === undefined || exception.line === item.line),
		);
		if (index < 0) return true;
		used.add(index);
		return false;
	});
	const stale = exceptions.filter((_, index) => !used.has(index));
	if (stale.length > 0) {
		throw new Error(
			`Stale OqtoUI exception(s) must be removed: ${stale.map((item) => `${item.file} [${item.rule}]`).join(", ")}`,
		);
	}
	return { violations: remaining, approvedExceptions: used.size };
}

function isExported(node) {
	return Boolean(
		node.modifiers?.some(
			(modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword,
		),
	);
}

function objectInterfaceSize(initializer) {
	if (!ts.isObjectLiteralExpression(initializer)) return 0;
	return initializer.properties.length;
}

function isOperationMember(member) {
	return (
		ts.isMethodSignature(member) ||
		ts.isCallSignatureDeclaration(member) ||
		(ts.isPropertySignature(member) &&
			member.type &&
			ts.isFunctionTypeNode(member.type))
	);
}

function operationExportCount(sourceFile) {
	let count = 0;
	for (const statement of sourceFile.statements) {
		if (ts.isExportDeclaration(statement)) {
			if (
				!statement.exportClause ||
				ts.isNamespaceExport(statement.exportClause)
			) {
				return Number.POSITIVE_INFINITY;
			}
			if (ts.isNamedExports(statement.exportClause) && !statement.isTypeOnly) {
				count += statement.exportClause.elements.filter(
					(element) => !element.isTypeOnly,
				).length;
			}
			continue;
		}
		if (!isExported(statement)) continue;
		if (ts.isInterfaceDeclaration(statement)) {
			count += statement.members.filter(isOperationMember).length;
			continue;
		}
		if (
			ts.isTypeAliasDeclaration(statement) &&
			ts.isTypeLiteralNode(statement.type)
		) {
			count += statement.type.members.filter(isOperationMember).length;
			continue;
		}
		if (
			ts.isFunctionDeclaration(statement) ||
			ts.isClassDeclaration(statement)
		) {
			count += 1;
			continue;
		}
		if (ts.isVariableStatement(statement)) {
			for (const declaration of statement.declarationList.declarations) {
				if (!declaration.initializer) continue;
				if (
					ts.isArrowFunction(declaration.initializer) ||
					ts.isFunctionExpression(declaration.initializer)
				) {
					count += 1;
				} else {
					count += objectInterfaceSize(declaration.initializer);
				}
			}
		}
	}
	return count;
}

function isOptionsBagName(name) {
	return /^(args|config|input|options|opts|params|payload)$/.test(name);
}

function hasUnstructuredOptionsBag(node) {
	if (!node.parameters) return false;
	return node.parameters.some((parameter) => {
		const type = parameter.type;
		if (type && ts.isTypeLiteralNode(type)) return true;
		if (!ts.isIdentifier(parameter.name)) return false;
		if (!isOptionsBagName(parameter.name.text)) return false;
		if (!type) return true;
		if (
			type.kind === ts.SyntaxKind.AnyKeyword ||
			type.kind === ts.SyntaxKind.UnknownKeyword ||
			type.kind === ts.SyntaxKind.ObjectKeyword
		) {
			return true;
		}
		return ts.isTypeReferenceNode(type) && type.typeName.getText() === "Record";
	});
}

function oversizedOptionsBag(node, typeContext) {
	if (!node.parameters) return null;
	for (const parameter of node.parameters) {
		if (
			!ts.isIdentifier(parameter.name) ||
			!isOptionsBagName(parameter.name.text)
		) {
			continue;
		}
		const fields = typeMemberCount(parameter.type, typeContext);
		if (fields !== null && fields > 8) {
			return { name: parameter.name.text, fields };
		}
	}
	return null;
}

function nonCommentLineCount(sourceFile, scriptKind) {
	const scanner = ts.createScanner(
		ts.ScriptTarget.Latest,
		true,
		scriptKind,
		sourceFile.text,
	);
	const lines = new Set();
	let token = scanner.scan();
	while (token !== ts.SyntaxKind.EndOfFileToken) {
		lines.add(
			sourceFile.getLineAndCharacterOfPosition(scanner.getTokenPos()).line + 1,
		);
		token = scanner.scan();
	}
	return lines.size;
}

function literalText(node) {
	if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
		return node.text;
	}
	return null;
}

function containsUserText(value) {
	return /\p{L}{2,}/u.test(value.trim());
}

function propertyNameText(name) {
	if (!name) return null;
	if (ts.isIdentifier(name) || ts.isStringLiteral(name)) return name.text;
	return null;
}

function callName(node) {
	if (!ts.isCallExpression(node) && !ts.isNewExpression(node)) return null;
	const expression = node.expression;
	if (ts.isIdentifier(expression)) return expression.text;
	if (ts.isPropertyAccessExpression(expression)) return expression.name.text;
	return null;
}

function findTypeDeclaration(sourceFile, name) {
	return sourceFile.statements.find(
		(statement) =>
			(ts.isInterfaceDeclaration(statement) ||
				ts.isTypeAliasDeclaration(statement)) &&
			statement.name.text === name,
	);
}

function declarationMemberCount(declaration) {
	if (declaration && ts.isInterfaceDeclaration(declaration)) {
		return declaration.members.length;
	}
	if (
		declaration &&
		ts.isTypeAliasDeclaration(declaration) &&
		ts.isTypeLiteralNode(declaration.type)
	) {
		return declaration.type.members.length;
	}
	return null;
}

function importedTypeDeclaration(typeName, typeContext) {
	for (const statement of typeContext.sourceFile.statements) {
		if (
			!ts.isImportDeclaration(statement) ||
			!ts.isStringLiteral(statement.moduleSpecifier) ||
			!statement.importClause?.namedBindings ||
			!ts.isNamedImports(statement.importClause.namedBindings)
		) {
			continue;
		}
		const imported = statement.importClause.namedBindings.elements.find(
			(element) => element.name.text === typeName,
		);
		if (!imported) continue;
		const target = resolveInternalImport(
			typeContext.sourceRoot,
			typeContext.absolutePath,
			statement.moduleSpecifier.text,
			typeContext.allFiles,
		);
		if (!target) return null;
		const targetSource = typeContext.sourceFiles.get(target);
		if (!targetSource) return null;
		return findTypeDeclaration(
			targetSource,
			imported.propertyName?.text ?? imported.name.text,
		);
	}
	return null;
}

function typeMemberCount(typeNode, typeContext) {
	if (!typeNode) return null;
	if (ts.isTypeLiteralNode(typeNode)) return typeNode.members.length;
	if (!ts.isTypeReferenceNode(typeNode)) return null;
	if (typeNode.typeArguments?.length) {
		const wrapped = typeMemberCount(typeNode.typeArguments.at(-1), typeContext);
		if (wrapped !== null) return wrapped;
	}
	if (!ts.isIdentifier(typeNode.typeName)) return null;
	const local = findTypeDeclaration(
		typeContext.sourceFile,
		typeNode.typeName.text,
	);
	return declarationMemberCount(
		local ?? importedTypeDeclaration(typeNode.typeName.text, typeContext),
	);
}

function wrappedFunction(expression) {
	if (ts.isArrowFunction(expression) || ts.isFunctionExpression(expression)) {
		return expression;
	}
	if (ts.isCallExpression(expression)) {
		for (const argument of expression.arguments) {
			const found = wrappedFunction(argument);
			if (found) return found;
		}
	}
	return null;
}

function componentProps(node, typeContext) {
	let name = null;
	let parameterType = null;
	if (
		ts.isFunctionDeclaration(node) &&
		node.name &&
		/^[A-Z]/.test(node.name.text)
	) {
		name = node.name.text;
		parameterType = node.parameters[0]?.type ?? null;
	} else if (
		ts.isVariableDeclaration(node) &&
		ts.isIdentifier(node.name) &&
		/^[A-Z]/.test(node.name.text) &&
		node.initializer
	) {
		name = node.name.text;
		const implementation = wrappedFunction(node.initializer);
		parameterType = implementation?.parameters[0]?.type ?? null;
		if (
			!parameterType &&
			ts.isCallExpression(node.initializer) &&
			node.initializer.typeArguments?.length
		) {
			parameterType = node.initializer.typeArguments.at(-1);
		}
		if (
			!parameterType &&
			node.type &&
			ts.isTypeReferenceNode(node.type) &&
			node.type.typeArguments?.length
		) {
			parameterType = node.type.typeArguments.at(-1);
		}
	}
	if (!name || !parameterType) return null;
	return { name, fields: typeMemberCount(parameterType, typeContext) };
}

function isInsideTrans(node) {
	let current = node.parent;
	while (current) {
		if (ts.isJsxElement(current)) {
			const tag = current.openingElement.tagName.getText();
			if (tag === "Trans" || tag.endsWith(".Trans")) return true;
		}
		current = current.parent;
	}
	return false;
}

function inspectFile(
	sourceRoot,
	absolutePath,
	sourceFile,
	allFiles,
	sourceFiles,
) {
	const relative = normalizedRelative(sourceRoot, absolutePath);
	const layer = sourceLayer(sourceRoot, absolutePath);
	const violations = [];
	const edges = [];
	const typeContext = {
		sourceRoot,
		absolutePath,
		sourceFile,
		allFiles,
		sourceFiles,
	};

	if (!layer) {
		violations.push(
			violation(
				"architecture/unknown-layer",
				relative,
				null,
				sourceFile,
				`Source must live in one of: ${LAYERS.join(", ")}`,
			),
		);
	}

	const lines = nonCommentLineCount(
		sourceFile,
		absolutePath.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
	);
	const limit = absolutePath.endsWith(".tsx") ? 300 : 400;
	if (lines > limit) {
		violations.push(
			violation(
				"budget/source-lines",
				relative,
				null,
				sourceFile,
				`${lines} non-comment source lines exceeds the ${limit}-line limit`,
			),
		);
	}

	const exportedOperations = operationExportCount(sourceFile);
	if (exportedOperations > 7) {
		violations.push(
			violation(
				"budget/public-operations",
				relative,
				null,
				sourceFile,
				`${exportedOperations} exported operations exceeds the limit of 7`,
			),
		);
	}

	function visit(node) {
		const moduleSpecifier =
			(ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
			node.moduleSpecifier &&
			ts.isStringLiteral(node.moduleSpecifier)
				? node.moduleSpecifier.text
				: null;
		if (moduleSpecifier) {
			if (
				ts.isImportDeclaration(node) &&
				node.importClause?.namedBindings &&
				ts.isNamespaceImport(node.importClause.namedBindings)
			) {
				violations.push(
					violation(
						"architecture/broad-import",
						relative,
						node,
						sourceFile,
						"Namespace imports are forbidden; import the narrow interface explicitly",
					),
				);
			}
			if (isLegacyImport(moduleSpecifier)) {
				violations.push(
					violation(
						"architecture/legacy-import",
						relative,
						node,
						sourceFile,
						`Legacy import is forbidden: ${moduleSpecifier}`,
					),
				);
			}
			if (moduleSpecifier.startsWith(".")) {
				const target = path.resolve(
					path.dirname(absolutePath),
					moduleSpecifier,
				);
				if (!isInside(sourceRoot, target)) {
					violations.push(
						violation(
							"architecture/source-boundary-import",
							relative,
							node,
							sourceFile,
							"Relative imports may not escape frontend/src/oqto-ui",
						),
					);
				}
			}
			if (
				moduleSpecifier.startsWith("@/") &&
				!moduleSpecifier.startsWith("@/src/oqto-ui/") &&
				!projectAliasAllowed(layer, moduleSpecifier)
			) {
				violations.push(
					violation(
						"architecture/project-alias-import",
						relative,
						node,
						sourceFile,
						`Project alias requires an explicit adapter allowlist entry: ${moduleSpecifier}`,
					),
				);
			}
			if (
				ts.isExportDeclaration(node) &&
				(!node.exportClause || ts.isNamespaceExport(node.exportClause))
			) {
				violations.push(
					violation(
						"architecture/broad-barrel",
						relative,
						node,
						sourceFile,
						"export * barrels are forbidden",
					),
				);
			}

			const resolved = resolveInternalImport(
				sourceRoot,
				absolutePath,
				moduleSpecifier,
				allFiles,
			);
			if (resolved) {
				edges.push(resolved);
				const targetLayer = sourceLayer(sourceRoot, resolved);
				if (
					layer &&
					targetLayer &&
					!ALLOWED_DIRECT_IMPORTS[layer].has(targetLayer)
				) {
					violations.push(
						violation(
							"architecture/layer-direction",
							relative,
							node,
							sourceFile,
							`${layer} may not import ${targetLayer} directly`,
						),
					);
				}
				if (
					layer &&
					targetLayer === layer &&
					moduleArea(sourceRoot, absolutePath) !==
						moduleArea(sourceRoot, resolved)
				) {
					violations.push(
						violation(
							"architecture/sideways-import",
							relative,
							node,
							sourceFile,
							`Cross-${layer} imports are forbidden; compose through the owning higher layer`,
						),
					);
				}
			}
		}

		if (
			ts.isInterfaceDeclaration(node) &&
			node.name.text.endsWith("Props") &&
			node.members.length > 8
		) {
			violations.push(
				violation(
					"budget/props",
					relative,
					node,
					sourceFile,
					`${node.name.text} has ${node.members.length} fields; limit is 8`,
				),
			);
		}
		if (
			ts.isTypeAliasDeclaration(node) &&
			node.name.text.endsWith("Props") &&
			ts.isTypeLiteralNode(node.type) &&
			node.type.members.length > 8
		) {
			violations.push(
				violation(
					"budget/props",
					relative,
					node,
					sourceFile,
					`${node.name.text} has ${node.type.members.length} fields; limit is 8`,
				),
			);
		}
		const props = componentProps(node, typeContext);
		if (props?.fields > 8) {
			violations.push(
				violation(
					"budget/props",
					relative,
					node,
					sourceFile,
					`${props.name} receives ${props.fields} prop fields; limit is 8`,
				),
			);
		}

		const isFunctionLike =
			ts.isFunctionDeclaration(node) ||
			ts.isMethodDeclaration(node) ||
			ts.isArrowFunction(node) ||
			ts.isFunctionExpression(node);
		if (isFunctionLike && hasUnstructuredOptionsBag(node)) {
			violations.push(
				violation(
					"budget/unstructured-options",
					relative,
					node,
					sourceFile,
					"Options/config/args bags require a named structured type",
				),
			);
		}
		const oversizedOptions = isFunctionLike
			? oversizedOptionsBag(node, typeContext)
			: null;
		if (oversizedOptions) {
			violations.push(
				violation(
					"budget/options-fields",
					relative,
					node,
					sourceFile,
					`${oversizedOptions.name} has ${oversizedOptions.fields} fields; limit is 8`,
				),
			);
		}

		const invokedName = callName(node);
		if (invokedName === "useEffect") {
			violations.push(
				violation(
					"react/raw-use-effect",
					relative,
					node,
					sourceFile,
					"Raw useEffect is forbidden; use an approved lifecycle module",
				),
			);
		}
		if (invokedName === "CustomEvent") {
			violations.push(
				violation(
					"browser/custom-event",
					relative,
					node,
					sourceFile,
					"Window/DOM custom-event coordination is forbidden",
				),
			);
		}
		if (
			invokedName &&
			ADAPTER_ONLY_GLOBALS.has(invokedName) &&
			layer !== "platform" &&
			!(layer === "dev" && SCRIPTED_ADAPTER_PATTERN.test(absolutePath))
		) {
			violations.push(
				violation(
					"browser/adapter-only-api",
					relative,
					node,
					sourceFile,
					`${invokedName} may only be used in platform adapters`,
				),
			);
		}

		if (ts.isIdentifier(node) && ADAPTER_ONLY_GLOBALS.has(node.text)) {
			const handledByParent =
				ts.isImportSpecifier(node.parent) ||
				ts.isImportClause(node.parent) ||
				(ts.isPropertyAccessExpression(node.parent) &&
					node.parent.name === node) ||
				((ts.isCallExpression(node.parent) ||
					ts.isNewExpression(node.parent)) &&
					node.parent.expression === node);
			if (
				!handledByParent &&
				layer !== "platform" &&
				!(layer === "dev" && SCRIPTED_ADAPTER_PATTERN.test(absolutePath))
			) {
				violations.push(
					violation(
						"browser/adapter-only-api",
						relative,
						node,
						sourceFile,
						`${node.text} may only be used in platform adapters`,
					),
				);
			}
		}

		if (
			ts.isPropertyAccessExpression(node) &&
			ts.isIdentifier(node.expression) &&
			["window", "globalThis"].includes(node.expression.text) &&
			ADAPTER_ONLY_GLOBALS.has(node.name.text) &&
			layer !== "platform" &&
			!(layer === "dev" && SCRIPTED_ADAPTER_PATTERN.test(absolutePath))
		) {
			violations.push(
				violation(
					"browser/adapter-only-api",
					relative,
					node,
					sourceFile,
					`${node.expression.text}.${node.name.text} may only be used in platform adapters`,
				),
			);
		}
		if (
			ts.isPropertyAccessExpression(node) &&
			ts.isIdentifier(node.expression) &&
			node.expression.text === "navigator" &&
			node.name.text === "serviceWorker" &&
			layer !== "platform"
		) {
			violations.push(
				violation(
					"browser/adapter-only-api",
					relative,
					node,
					sourceFile,
					"navigator.serviceWorker may only be used in platform adapters",
				),
			);
		}
		if (
			ts.isPropertyAccessExpression(node) &&
			ts.isIdentifier(node.expression) &&
			node.expression.text === "document" &&
			["querySelector", "querySelectorAll"].includes(node.name.text)
		) {
			violations.push(
				violation(
					"browser/dom-query-coordination",
					relative,
					node,
					sourceFile,
					"DOM-query coordination is forbidden",
				),
			);
		}

		if (
			ts.isTypeReferenceNode(node) &&
			node.typeArguments?.length === 2 &&
			node.typeArguments[0].kind === ts.SyntaxKind.StringKeyword &&
			(node.typeArguments[1].kind === ts.SyntaxKind.UnknownKeyword ||
				node.typeArguments[1].kind === ts.SyntaxKind.AnyKeyword)
		) {
			violations.push(
				violation(
					"types/record-unknown",
					relative,
					node,
					sourceFile,
					`${node.typeName.getText()}<string, unknown/any> erases key knowledge; model the expected keys explicitly`,
				),
			);
		}

		if (node.kind === ts.SyntaxKind.UnknownKeyword && layer !== "platform") {
			let current = node.parent;
			while (current && !ts.isSourceFile(current) && !ts.isBlock(current)) {
				const modifiers = ts.canHaveModifiers(current)
					? ts.getModifiers(current)
					: undefined;
				if (
					modifiers?.some(
						(modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword,
					)
				) {
					violations.push(
						violation(
							"types/exported-unknown",
							relative,
							node,
							sourceFile,
							"Exported signatures outside platform adapters must not expose 'unknown'; narrow at the adapter boundary",
						),
					);
					break;
				}
				current = current.parent;
			}
		}

		if (
			ts.isIndexSignatureDeclaration(node) &&
			node.type &&
			(node.type.kind === ts.SyntaxKind.UnknownKeyword ||
				node.type.kind === ts.SyntaxKind.AnyKeyword)
		) {
			violations.push(
				violation(
					"types/record-unknown",
					relative,
					node,
					sourceFile,
					"Index signatures with unknown/any values erase key knowledge; model the expected keys explicitly",
				),
			);
		}

		if (ts.isJsxAttribute(node) && node.name.text === "style") {
			// Custom properties only (e.g. measured layout values consumed by
			// stylesheet rules) are acceptable; visual declarations stay in CSS.
			if (!jsxStyleUsesOnlyCustomProperties(node)) {
				violations.push(
					violation(
						"design/inline-style",
						relative,
						node,
						sourceFile,
						"Inline styles are forbidden in OqtoUI views (custom properties like --var are allowed)",
					),
				);
			}
		}

		const value = literalText(node);
		if (value !== null) {
			for (const visual of VISUAL_VALUE_PATTERNS) {
				if (visual.pattern.test(value)) {
					violations.push(
						violation(
							"design/hardcoded-visual",
							relative,
							node,
							sourceFile,
							`Hardcoded ${visual.name} is forbidden; use a design-system role/variant`,
						),
					);
					break;
				}
			}
		}

		if (
			value !== null &&
			ts.isJsxExpression(node.parent) &&
			containsUserText(value) &&
			!isInsideTrans(node)
		) {
			violations.push(
				violation(
					"i18n/untranslated-text",
					relative,
					node,
					sourceFile,
					"User-facing JSX text must come from i18next",
				),
			);
		}
		if (
			ts.isJsxText(node) &&
			containsUserText(node.text) &&
			!isInsideTrans(node)
		) {
			violations.push(
				violation(
					"i18n/untranslated-text",
					relative,
					node,
					sourceFile,
					"User-facing JSX text must come from i18next",
				),
			);
		}
		if (
			ts.isJsxAttribute(node) &&
			USER_FACING_JSX_ATTRIBUTES.has(node.name.text) &&
			node.initializer &&
			ts.isStringLiteral(node.initializer) &&
			containsUserText(node.initializer.text)
		) {
			violations.push(
				violation(
					"i18n/untranslated-attribute",
					relative,
					node,
					sourceFile,
					`${node.name.text} must come from i18next`,
				),
			);
		}
		if (
			ts.isPropertyAssignment(node) &&
			USER_FACING_OBJECT_PROPERTIES.has(propertyNameText(node.name)) &&
			literalText(node.initializer) !== null &&
			containsUserText(literalText(node.initializer))
		) {
			violations.push(
				violation(
					"i18n/untranslated-property",
					relative,
					node,
					sourceFile,
					`${propertyNameText(node.name)} must come from i18next`,
				),
			);
		}

		ts.forEachChild(node, visit);
	}

	visit(sourceFile);
	return { violations, edges };
}

function cycleViolations(sourceRoot, graph, sourceFiles) {
	const violations = [];
	const state = new Map();
	const stack = [];
	const reported = new Set();

	function visit(file) {
		state.set(file, 1);
		stack.push(file);
		for (const target of graph.get(file) ?? []) {
			if (!graph.has(target)) continue;
			if (!state.has(target)) {
				visit(target);
			} else if (state.get(target) === 1) {
				const start = stack.indexOf(target);
				const cycle = [...stack.slice(start), target];
				const key = [...new Set(cycle)].sort().join("|");
				if (!reported.has(key)) {
					reported.add(key);
					const relative = normalizedRelative(sourceRoot, file);
					violations.push(
						violation(
							"architecture/import-cycle",
							relative,
							null,
							sourceFiles.get(file),
							`Import cycle: ${cycle.map((item) => normalizedRelative(sourceRoot, item)).join(" -> ")}`,
						),
					);
				}
			}
		}
		stack.pop();
		state.set(file, 2);
	}

	for (const file of graph.keys()) {
		if (!state.has(file)) visit(file);
	}
	return violations;
}

export async function inspectOqtoUI(
	sourceRoot,
	exceptionsPath = DEFAULT_EXCEPTIONS_PATH,
) {
	const absoluteRoot = path.resolve(sourceRoot);
	const exceptions = await readExceptions(exceptionsPath);
	const files = await walk(absoluteRoot);
	const allFiles = new Set(files);
	const sourceFiles = new Map();
	for (const file of files) {
		const text = await fs.readFile(file, "utf8");
		sourceFiles.set(
			file,
			ts.createSourceFile(
				file,
				text,
				ts.ScriptTarget.Latest,
				true,
				file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
			),
		);
	}

	const graph = new Map();
	const violations = [];
	for (const file of files) {
		const result = inspectFile(
			absoluteRoot,
			file,
			sourceFiles.get(file),
			allFiles,
			sourceFiles,
		);
		graph.set(file, result.edges);
		violations.push(...result.violations);
	}
	violations.push(...cycleViolations(absoluteRoot, graph, sourceFiles));

	violations.sort(
		(a, b) =>
			a.file.localeCompare(b.file) ||
			a.line - b.line ||
			a.column - b.column ||
			a.rule.localeCompare(b.rule),
	);
	const filtered = applyExceptions(violations, exceptions);
	return {
		sourceRoot: absoluteRoot,
		files: files.length,
		violations: filtered.violations,
		approvedExceptions: filtered.approvedExceptions,
	};
}

async function main() {
	const { sourceRoot, exceptionsPath, json } = parseArgs(process.argv.slice(2));
	const result = await inspectOqtoUI(sourceRoot, exceptionsPath);
	if (json) {
		console.log(JSON.stringify(result, null, 2));
	} else if (result.violations.length === 0) {
		const baseline =
			result.approvedExceptions === 0
				? "zero-baseline"
				: `${result.approvedExceptions} owner-approved exception(s)`;
		console.log(
			`OqtoUI guardrails OK (${result.files} source files, ${baseline}).`,
		);
	} else {
		console.error(
			`OqtoUI guardrails failed with ${result.violations.length} violation(s):`,
		);
		for (const item of result.violations) {
			console.error(
				`  ${item.file}:${item.line}:${item.column} [${item.rule}] ${item.message}`,
			);
		}
	}
	if (result.violations.length > 0) process.exitCode = 1;
}

const invokedAsScript =
	process.argv[1] &&
	path.resolve(process.argv[1]) ===
		path.resolve(new URL(import.meta.url).pathname);
if (invokedAsScript) {
	main().catch((error) => {
		console.error(
			error instanceof Error ? (error.stack ?? error.message) : String(error),
		);
		process.exit(1);
	});
}
