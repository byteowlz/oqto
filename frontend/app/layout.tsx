import type React from "react"
import type { Metadata } from "next"
import "./globals.css"

export const metadata: Metadata = {
  title: "AI Agent Workspace Platform",
  description: "Secure, scalable platform for AI agent collaboration and workspace management",
}

function RootLayoutInner({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="de" className="dark" suppressHydrationWarning>
      <body className="font-mono antialiased bg-background text-foreground">
        {children}
      </body>
    </html>
  )
}

export default function RootLayout(props: { children: React.ReactNode }) {
  return <RootLayoutInner {...props} />
}
