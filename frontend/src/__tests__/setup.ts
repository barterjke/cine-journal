/**
 * Vitest setup. Registers the jest-dom matchers and their types.
 *
 * Nothing here imports from `src/`. A setup file runs before the test file's hoisted
 * `vi.mock` calls, so an app module imported here would be the unmocked copy. Fixtures
 * and the API stub live beside the tests instead.
 *
 * Unmounting is left to Testing Library, which installs its own `afterEach` because
 * `globals` is on.
 */
import '@testing-library/jest-dom/vitest'
