import { getRequestConfig } from "next-intl/server"
import { cookies } from "next/headers"

export type Locale = "en" | "de"

export const locales: Locale[] = ["en", "de"]
export const defaultLocale: Locale = "de"
export const LOCALE_COOKIE_NAME = "NEXT_LOCALE"

export default getRequestConfig(async () => {
  // Try to get locale from cookie
  const cookieStore = await cookies()
  const cookieLocale = cookieStore.get(LOCALE_COOKIE_NAME)?.value

  // Validate that the incoming `locale` parameter is valid
  let locale: Locale = defaultLocale
  if (cookieLocale && locales.includes(cookieLocale as Locale)) {
    locale = cookieLocale as Locale
  }

  return {
    locale,
    messages: (await import(`../messages/${locale}.json`)).default,
  }
})
