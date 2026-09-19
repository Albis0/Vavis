/**
 * Test environment for the React components.
 *
 * `jest-dom` adds the matchers the component tests read best with
 * (`toBeVisible`, `toHaveTextContent`), and the cleanup below unmounts
 * anything a test rendered -- without it a component's effects keep running
 * into the next test, which shows up as a passing suite that fails when the
 * file order changes.
 */
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(cleanup);
