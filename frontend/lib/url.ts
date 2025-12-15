export function toAbsoluteWsUrl(value: string): string {
  if (!value) return ""
  if (value.startsWith("ws://") || value.startsWith("wss://")) return value
  if (value.startsWith("http://") || value.startsWith("https://")) return value.replace(/^http/, "ws")
  if (typeof window === "undefined") return value
  const scheme = window.location.protocol === "https:" ? "wss:" : "ws:"
  const path = value.startsWith("/") ? value : `/${value}`
  return `${scheme}//${window.location.host}${path}`
}

