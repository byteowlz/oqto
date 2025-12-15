import { describe, it, expect } from "vitest"
import {
  getFileExtension,
  getFileTypeInfo,
  isTextFile,
  isBinaryFile,
  getSyntaxLanguage,
} from "../lib/file-types"

describe("file-types", () => {
  describe("getFileExtension", () => {
    it("extracts simple extensions", () => {
      expect(getFileExtension("file.txt")).toBe("txt")
      expect(getFileExtension("script.js")).toBe("js")
      expect(getFileExtension("component.tsx")).toBe("tsx")
    })

    it("handles multiple dots", () => {
      expect(getFileExtension("file.test.ts")).toBe("ts")
      expect(getFileExtension("archive.tar.gz")).toBe("gz")
    })

    it("handles files without extension", () => {
      expect(getFileExtension("README")).toBe("")
      expect(getFileExtension("file")).toBe("")
    })

    it("recognizes special files", () => {
      expect(getFileExtension("Dockerfile")).toBe("dockerfile")
      expect(getFileExtension("Makefile")).toBe("makefile")
      expect(getFileExtension(".gitignore")).toBe("gitignore")
      expect(getFileExtension(".env")).toBe("env")
    })

    it("is case insensitive", () => {
      expect(getFileExtension("FILE.TXT")).toBe("txt")
      expect(getFileExtension("README.MD")).toBe("md")
    })
  })

  describe("getFileTypeInfo", () => {
    it("detects code files", () => {
      const info = getFileTypeInfo("app.ts")
      expect(info.category).toBe("code")
      expect(info.language).toBe("typescript")
    })

    it("detects markdown files", () => {
      const info = getFileTypeInfo("README.md")
      expect(info.category).toBe("markdown")
      expect(info.language).toBe("markdown")
    })

    it("detects image files", () => {
      const pngInfo = getFileTypeInfo("logo.png")
      expect(pngInfo.category).toBe("image")
      expect(pngInfo.mimeType).toBe("image/png")

      const svgInfo = getFileTypeInfo("icon.svg")
      expect(svgInfo.category).toBe("image")
      expect(svgInfo.mimeType).toBe("image/svg+xml")
    })

    it("detects CSV files", () => {
      const info = getFileTypeInfo("data.csv")
      expect(info.category).toBe("csv")
      expect(info.language).toBe("csv")
    })

    it("detects JSON files", () => {
      const info = getFileTypeInfo("config.json")
      expect(info.category).toBe("json")
      expect(info.language).toBe("json")
    })

    it("detects YAML files", () => {
      const yamlInfo = getFileTypeInfo("docker-compose.yaml")
      expect(yamlInfo.category).toBe("yaml")

      const ymlInfo = getFileTypeInfo("config.yml")
      expect(ymlInfo.category).toBe("yaml")
    })

    it("detects binary files", () => {
      const pdfInfo = getFileTypeInfo("document.pdf")
      expect(pdfInfo.category).toBe("pdf")

      const zipInfo = getFileTypeInfo("archive.zip")
      expect(zipInfo.category).toBe("binary")

      const exeInfo = getFileTypeInfo("program.exe")
      expect(exeInfo.category).toBe("binary")
    })

    it("handles unknown extensions as text", () => {
      const info = getFileTypeInfo("file.xyz")
      expect(info.category).toBe("unknown")
      expect(info.language).toBe("text")
    })
  })

  describe("isTextFile", () => {
    it("returns true for text-based files", () => {
      expect(isTextFile("code.ts")).toBe(true)
      expect(isTextFile("readme.md")).toBe(true)
      expect(isTextFile("data.json")).toBe(true)
      expect(isTextFile("notes.txt")).toBe(true)
    })

    it("returns false for binary files", () => {
      expect(isTextFile("image.png")).toBe(false)
      expect(isTextFile("doc.pdf")).toBe(false)
      expect(isTextFile("archive.zip")).toBe(false)
    })
  })

  describe("isBinaryFile", () => {
    it("returns true for binary files", () => {
      expect(isBinaryFile("image.jpg")).toBe(true)
      expect(isBinaryFile("doc.pdf")).toBe(true)
      expect(isBinaryFile("archive.tar.gz")).toBe(true)
    })

    it("returns false for text files", () => {
      expect(isBinaryFile("script.py")).toBe(false)
      expect(isBinaryFile("config.toml")).toBe(false)
    })
  })

  describe("getSyntaxLanguage", () => {
    it("returns correct language for known extensions", () => {
      expect(getSyntaxLanguage("app.tsx")).toBe("tsx")
      expect(getSyntaxLanguage("main.rs")).toBe("rust")
      expect(getSyntaxLanguage("script.py")).toBe("python")
      expect(getSyntaxLanguage("main.go")).toBe("go")
    })

    it("returns text for unknown extensions", () => {
      expect(getSyntaxLanguage("file.unknown")).toBe("text")
    })
  })
})
