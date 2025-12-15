import type React from "react"
import type { Metadata } from "next"
import { JetBrains_Mono } from "next/font/google"
import "./globals.css"
import { ClientOnly } from "@/components/client-only"

const jetbrainsMono = JetBrains_Mono({ subsets: ["latin"] })

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
      <body className={`${jetbrainsMono.className} antialiased bg-background text-foreground`}>
        <ClientOnly>
          {children}
        </ClientOnly>
      </body>
    </html>
  )
}

export default function RootLayout(props: { children: React.ReactNode }) {
  return <RootLayoutInner {...props} />
}
