import type { ComponentType } from "react"

export interface AppDefinition {
  id: string
  label: string
  description?: string
  component: ComponentType
  routes?: string[]
  permissions?: string[]
  defaultState?: Record<string, unknown>
  priority?: number
}

export class AppRegistry {
  private readonly apps = new Map<string, AppDefinition>()

  register(definition: AppDefinition) {
    this.apps.set(definition.id, definition)
    return this
  }

  getApp(id: string) {
    return this.apps.get(id)
  }

  getAllApps() {
    return Array.from(this.apps.values()).sort((a, b) => {
      const weightA = a.priority ?? Number.MAX_SAFE_INTEGER
      const weightB = b.priority ?? Number.MAX_SAFE_INTEGER
      if (weightA !== weightB) {
        return weightA - weightB
      }
      return a.label.localeCompare(b.label)
    })
  }
}

export const appRegistry = new AppRegistry()
