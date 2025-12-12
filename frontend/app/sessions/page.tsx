"use client"

import { SessionsApp } from "@/apps/sessions"
import { AppProvider } from "@/components/app-context"

export default function SessionsPage() {
  return (
    <AppProvider>
      <div className="p-6">
        <SessionsApp />
      </div>
    </AppProvider>
  )
}
