"use client"

import { AdminApp } from "@/apps/admin"
import { AppProvider } from "@/components/app-context"

export default function AdminPage() {
  return (
    <AppProvider>
      <div className="p-6">
        <AdminApp />
      </div>
    </AppProvider>
  )
}
