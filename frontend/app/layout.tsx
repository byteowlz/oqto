import type React from "react"
import type { Metadata, Viewport } from "next"
import { getLocale, getMessages } from "next-intl/server"
import { Providers } from "@/components/providers"
import "./globals.css"

export const metadata: Metadata = {
  title: "octo - got tentacles?",
  description: "Secure, scalable platform for AI agent collaboration and workspace management",
}

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  maximumScale: 1,
  userScalable: false,
}

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const locale = await getLocale()
  const messages = await getMessages()

  return (
    <html lang={locale} className="dark" suppressHydrationWarning>
      <body className="font-mono antialiased bg-background text-foreground">
        <Providers locale={locale} messages={messages}>
          {children}
        </Providers>
      </body>
    </html>
  )
}
