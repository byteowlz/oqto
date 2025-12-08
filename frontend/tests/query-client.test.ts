import { describe, it, expect, beforeEach, vi } from "vitest"
import { makeQueryClient } from "../lib/query-client"

describe("query-client", () => {
  describe("makeQueryClient", () => {
    it("creates a new query client instance", () => {
      const client = makeQueryClient()
      expect(client).toBeDefined()
      expect(client.getDefaultOptions()).toBeDefined()
    })

    it("configures default stale time", () => {
      const client = makeQueryClient()
      const options = client.getDefaultOptions()
      expect(options.queries?.staleTime).toBe(60 * 1000)
    })

    it("disables refetch on window focus by default", () => {
      const client = makeQueryClient()
      const options = client.getDefaultOptions()
      expect(options.queries?.refetchOnWindowFocus).toBe(false)
    })

    it("disables retry for mutations", () => {
      const client = makeQueryClient()
      const options = client.getDefaultOptions()
      expect(options.mutations?.retry).toBe(false)
    })
  })

  describe("getQueryClient", () => {
    beforeEach(() => {
      // Reset the singleton
      vi.resetModules()
    })

    it("returns the same instance on subsequent calls in browser", async () => {
      // Since we're in jsdom, window is defined
      const { getQueryClient: getClient } = await import("../lib/query-client")
      const client1 = getClient()
      const client2 = getClient()
      expect(client1).toBe(client2)
    })
  })
})
