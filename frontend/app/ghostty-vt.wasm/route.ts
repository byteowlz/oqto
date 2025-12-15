import { readFile } from "node:fs/promises"
import { join } from "node:path"
import { NextResponse } from "next/server"

export const runtime = "nodejs"

export async function GET() {
  const wasmPath = join(process.cwd(), "node_modules", "ghostty-web", "ghostty-vt.wasm")
  const wasm = await readFile(wasmPath)

  return new NextResponse(wasm, {
    headers: {
      "content-type": "application/wasm",
      "cache-control": "public, max-age=31536000, immutable",
    },
  })
}

