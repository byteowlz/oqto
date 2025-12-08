"use client"

import { WorkspacesApp } from "@/apps/workspaces"
import { AppProvider } from "@/components/app-context"

export default function WorkspacesPage() {
  return (
    <AppProvider>
      <div className="p-6">
        <WorkspacesApp />
      </div>
    </AppProvider>
  )
}
