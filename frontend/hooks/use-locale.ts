"use client"

import { useCallback, useTransition } from "react"
import { useLocale as useNextIntlLocale } from "next-intl"
import type { Locale } from "@/lib/i18n"

const LOCALE_COOKIE_NAME = "NEXT_LOCALE"

export function useLocale() {
  const currentLocale = useNextIntlLocale() as Locale
  const [isPending, startTransition] = useTransition()

  const setLocale = useCallback((newLocale: Locale) => {
    startTransition(() => {
      // Set cookie for persistence
      document.cookie = `${LOCALE_COOKIE_NAME}=${newLocale};path=/;max-age=31536000`
      // Reload to apply the new locale
      window.location.reload()
    })
  }, [])

  return {
    locale: currentLocale,
    setLocale,
    isPending,
  }
}
