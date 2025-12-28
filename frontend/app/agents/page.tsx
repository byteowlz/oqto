"use client"

import { PersonasApp } from "@/apps/personas"
import { AppProvider } from "@/components/app-context"

export default function AgentsPage() {
  return (
    <AppProvider>
      <div className="p-6">
        <PersonasApp />
      </div>
    </AppProvider>
  )
}
