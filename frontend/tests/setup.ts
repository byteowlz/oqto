import "@testing-library/jest-dom"
import { afterEach, vi } from "vitest"
import { cleanup } from "@testing-library/react"

// Cleanup after each test case
afterEach(() => {
  cleanup()
})

// Mock fetch globally
global.fetch = vi.fn()

// Mock localStorage
const localStorageMock = {
  getItem: vi.fn(),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
  length: 0,
  key: vi.fn(),
}
global.localStorage = localStorageMock as unknown as Storage
