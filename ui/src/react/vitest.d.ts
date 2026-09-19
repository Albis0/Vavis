/**
 * The jest-dom matchers, declared for TypeScript.
 *
 * `test-setup.ts` registers them at runtime with `expect.extend`, which the
 * compiler cannot see -- so without this every `toBeInTheDocument()` is a
 * type error even though the test passes. The usual fix is a `types` entry
 * for "@testing-library/jest-dom", but that pulls in the jest globals and
 * collides with vitest's own `expect`.
 */
import "vitest";
import type { TestingLibraryMatchers } from "@testing-library/jest-dom/matchers";

declare module "vitest" {
    interface Matchers<T = unknown>
        extends TestingLibraryMatchers<unknown, T> {}
}
