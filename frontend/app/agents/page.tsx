"use client"

import { AgentsApp } from "@/apps/agents"
import { AppProvider } from "@/components/app-context"

export default function AgentsPage() {
  return (
    <AppProvider>
        <div className="p-6">
        <AgentsApp />
        </div>
    </AppProvider>
  )
}
