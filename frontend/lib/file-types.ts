// File type detection and categorization utilities

export type FileCategory = "code" | "markdown" | "image" | "pdf" | "csv" | "json" | "yaml" | "xml" | "typst" | "text" | "binary" | "unknown"

export interface FileTypeInfo {
  extension: string
  category: FileCategory
  language?: string
  mimeType?: string
  icon?: string
}

// Extension to language mapping for syntax highlighting
const extensionToLanguage: Record<string, string> = {
  // JavaScript/TypeScript
  js: "javascript",
  jsx: "jsx",
  ts: "typescript",
  tsx: "tsx",
  mjs: "javascript",
  cjs: "javascript",

  // Web
  html: "html",
  htm: "html",
  css: "css",
  scss: "scss",
  sass: "sass",
  less: "less",
  vue: "vue",
  svelte: "svelte",

  // Data formats
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  xml: "xml",
  toml: "toml",
  csv: "csv",

  // Config files
  env: "bash",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  fish: "bash",
  ps1: "powershell",
  bat: "batch",
  cmd: "batch",

  // Programming languages
  py: "python",
  rb: "ruby",
  go: "go",
  rs: "rust",
  java: "java",
  kt: "kotlin",
  scala: "scala",
  c: "c",
  cpp: "cpp",
  h: "c",
  hpp: "cpp",
  cs: "csharp",
  swift: "swift",
  php: "php",
  pl: "perl",
  lua: "lua",
  r: "r",
  dart: "dart",
  elm: "elm",
  erl: "erlang",
  ex: "elixir",
  exs: "elixir",
  clj: "clojure",
  hs: "haskell",
  ml: "ocaml",
  fs: "fsharp",
  nim: "nim",
  zig: "zig",
  v: "v",
  cr: "crystal",

  // Markup/Document
  md: "markdown",
  mdx: "markdown",
  markdown: "markdown",
  rst: "restructuredtext",
  tex: "latex",
  latex: "latex",
  typ: "typst",

  // Database
  sql: "sql",
  graphql: "graphql",
  gql: "graphql",

  // DevOps
  dockerfile: "dockerfile",
  docker: "dockerfile",
  containerfile: "dockerfile",
  makefile: "makefile",
  cmake: "cmake",
  nginx: "nginx",
  tf: "hcl",
  hcl: "hcl",

  // Other
  diff: "diff",
  patch: "diff",
  proto: "protobuf",
  asm: "nasm",
}

// Image extensions
const imageExtensions = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "svg",
  "ico",
  "bmp",
  "tiff",
  "tif",
  "avif",
  "heic",
  "heif",
])

// Binary file extensions (not displayable as text)
const binaryExtensions = new Set([
  "doc",
  "docx",
  "xls",
  "xlsx",
  "ppt",
  "pptx",
  "zip",
  "tar",
  "gz",
  "bz2",
  "7z",
  "rar",
  "exe",
  "dll",
  "so",
  "dylib",
  "wasm",
  "bin",
  "dat",
  "db",
  "sqlite",
  "mp3",
  "mp4",
  "avi",
  "mov",
  "mkv",
  "wav",
  "flac",
  "ogg",
  "ttf",
  "otf",
  "woff",
  "woff2",
  "eot",
])

export function getFileExtension(filename: string): string {
  // Handle special files without extensions
  const lowerName = filename.toLowerCase()

  // Special case files
  const specialFiles: Record<string, string> = {
    dockerfile: "dockerfile",
    containerfile: "dockerfile",
    makefile: "makefile",
    cmakelists: "cmake",
    ".gitignore": "gitignore",
    ".gitattributes": "gitignore",
    ".dockerignore": "dockerignore",
    ".env": "env",
    ".env.local": "env",
    ".env.example": "env",
    ".prettierrc": "json",
    ".eslintrc": "json",
  }

  for (const [special, lang] of Object.entries(specialFiles)) {
    if (lowerName === special || lowerName.endsWith("/" + special)) {
      return lang
    }
  }

  const parts = filename.split(".")
  if (parts.length > 1) {
    return parts[parts.length - 1].toLowerCase()
  }
  return ""
}

export function getFileTypeInfo(filename: string): FileTypeInfo {
  const ext = getFileExtension(filename)
  const lowerExt = ext.toLowerCase()

  // Check for images
  if (imageExtensions.has(lowerExt)) {
    return {
      extension: ext,
      category: "image",
      mimeType: `image/${lowerExt === "svg" ? "svg+xml" : lowerExt}`,
    }
  }

  // Check for PDF
  if (lowerExt === "pdf") {
    return {
      extension: ext,
      category: "pdf",
      mimeType: "application/pdf",
    }
  }

  // Check for CSV
  if (lowerExt === "csv") {
    return {
      extension: ext,
      category: "csv",
      language: "csv",
    }
  }

  // Check for markdown
  if (["md", "mdx", "markdown"].includes(lowerExt)) {
    return {
      extension: ext,
      category: "markdown",
      language: "markdown",
    }
  }

  // Check for JSON
  if (lowerExt === "json") {
    return {
      extension: ext,
      category: "json",
      language: "json",
    }
  }

  // Check for YAML
  if (["yaml", "yml"].includes(lowerExt)) {
    return {
      extension: ext,
      category: "yaml",
      language: "yaml",
    }
  }

  // Check for XML
  if (lowerExt === "xml") {
    return {
      extension: ext,
      category: "xml",
      language: "xml",
    }
  }

  // Check for Typst
  if (lowerExt === "typ") {
    return {
      extension: ext,
      category: "typst",
      language: "typst",
    }
  }

  // Check for binary
  if (binaryExtensions.has(lowerExt)) {
    return {
      extension: ext,
      category: "binary",
    }
  }

  // Check for code with known language
  const language = extensionToLanguage[lowerExt]
  if (language) {
    return {
      extension: ext,
      category: "code",
      language,
    }
  }

  // Check for text files (no extension or common text extensions)
  if (ext === "" || ["txt", "log", "text"].includes(lowerExt)) {
    return {
      extension: ext,
      category: "text",
      language: "text",
    }
  }

  // Unknown extension - treat as text
  return {
    extension: ext,
    category: "unknown",
    language: "text",
  }
}

export function isTextFile(filename: string): boolean {
  const info = getFileTypeInfo(filename)
  return !["binary", "image", "pdf"].includes(info.category)
}

export function isBinaryFile(filename: string): boolean {
  const info = getFileTypeInfo(filename)
  return info.category === "binary" || info.category === "image" || info.category === "pdf"
}

export function getSyntaxLanguage(filename: string): string {
  const info = getFileTypeInfo(filename)
  return info.language || "text"
}
