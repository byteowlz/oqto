"use client"

import type React from "react"
import { QueryClientProvider } from "@tanstack/react-query"
import { NextIntlClientProvider, type AbstractIntlMessages } from "next-intl"
import { getQueryClient } from "@/lib/query-client"
import { ThemeProvider } from "@/components/theme-provider"

type ProvidersProps = {
  children: React.ReactNode
  locale: string
  messages: AbstractIntlMessages
}

export function Providers({ children, locale, messages }: ProvidersProps) {
  const queryClient = getQueryClient()

  return (
    <QueryClientProvider client={queryClient}>
      <NextIntlClientProvider locale={locale} messages={messages}>
        <ThemeProvider attribute="class" defaultTheme="dark" enableSystem disableTransitionOnChange>
          {children}
        </ThemeProvider>
      </NextIntlClientProvider>
    </QueryClientProvider>
  )
}
