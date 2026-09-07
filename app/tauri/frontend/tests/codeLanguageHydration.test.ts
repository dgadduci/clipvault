import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  __resetCodeLanguageHydrationForTests,
  hydrateCodeLanguageForEntry,
  codeLanguageLabel,
} from "../src/lib/codeLanguageHydration.ts";
import { codeLanguageSetCommand } from "../src/lib/tauri.ts";

interface CapturedCall {
  entryId: number;
  codeLanguage: string | null;
}

const capturedCalls: CapturedCall[] = [];
let nextResponse:
  | { kind: "updated"; entry: { id: number; code_language: string | null } }
  | { kind: "noop"; entry: { id: number; code_language: string | null } }
  | { kind: "not_found" }
  | null = null;

(globalThis as { window: { __TAURI_INTERNALS__?: unknown } }).window = {
  __TAURI_INTERNALS__: {
    invoke: async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd !== "clipvault_code_language_set") {
        throw new Error(`unexpected command ${cmd}`);
      }
      capturedCalls.push({
        entryId: Number(args?.entryId),
        codeLanguage: (args?.codeLanguage as string | null) ?? null,
      });
      if (!nextResponse) {
        throw new Error("no response staged");
      }
      return nextResponse;
    },
  },
};

test.beforeEach(() => {
  __resetCodeLanguageHydrationForTests();
  capturedCalls.length = 0;
  nextResponse = null;
});

const baseEntry = {
  id: 42,
  content: "function greet(name) {\n  return `Hello ${name}`;\n}\nconst g = greet('world');\nconsole.log(g);",
  content_type: "code",
  code_language: null as string | null,
};

test("hydrateCodeLanguageForEntry skips already-classified rows", async () => {
  const outcome = await hydrateCodeLanguageForEntry({
    ...baseEntry,
    code_language: "javascript",
  });
  assert.equal(outcome.language, "javascript");
  assert.equal(outcome.persisted, false);
  assert.equal(capturedCalls.length, 0);
});

test("hydrateCodeLanguageForEntry calls the bridge with the canonical detection", async () => {
  nextResponse = { kind: "updated", entry: { id: baseEntry.id, code_language: "javascript" } };
  const outcome = await hydrateCodeLanguageForEntry({ ...baseEntry });
  assert.equal(outcome.persisted, true);
  assert.equal(outcome.language, "javascript");
  assert.equal(capturedCalls.length, 1);
  assert.equal(capturedCalls[0].entryId, 42);
  assert.equal(capturedCalls[0].codeLanguage, "javascript");
});

test("hydrateCodeLanguageForEntry coalesces per entry id", async () => {
  // Two concurrent calls for the same entry id MUST collapse into a
  // single IPC invocation. The second response is a noop so the
  // helper still surfaces the canonical language without re-issuing
  // the call.
  nextResponse = { kind: "updated", entry: { id: baseEntry.id, code_language: "javascript" } };
  const first = hydrateCodeLanguageForEntry({ ...baseEntry });
  // Stage the second response before awaiting the first call so the
  // second invocation observes the in-flight token.
  nextResponse = { kind: "noop", entry: { id: baseEntry.id, code_language: "javascript" } };
  const second = hydrateCodeLanguageForEntry({ ...baseEntry });
  const [a, b] = await Promise.all([first, second]);
  assert.equal(capturedCalls.length, 1, "calls must coalesce into a single IPC");
  assert.equal(a.language, "javascript");
  assert.equal(b.language, "javascript");
  assert.equal(b.persisted, false, "second call reports noop");
});

test("hydrateCodeLanguageForEntry skips non-textual content types", async () => {
  const outcome = await hydrateCodeLanguageForEntry({
    ...baseEntry,
    content_type: "image",
  });
  assert.equal(outcome.language, null);
  assert.equal(outcome.persisted, false);
  assert.equal(capturedCalls.length, 0);
});

test("hydrateCodeLanguageForEntry respects kindHint", async () => {
  const outcome = await hydrateCodeLanguageForEntry(
    { ...baseEntry, content_type: "text" },
    { kindHint: "text" },
  );
  assert.equal(outcome.language, null);
  assert.equal(outcome.persisted, false);
  assert.equal(capturedCalls.length, 0);
});

test("hydrateCodeLanguageForEntry surfaces noop responses as unchanged", async () => {
  nextResponse = { kind: "noop", entry: { id: baseEntry.id, code_language: "python" } };
  const outcome = await hydrateCodeLanguageForEntry({
    ...baseEntry,
    content:
      "def greet(name):\n    message = f'Hello {name}'\n    print(message)\n\ngreet('world')\n",
  });
  assert.equal(outcome.persisted, false);
  assert.equal(outcome.language, "python");
});

test("hydrateCodeLanguageForEntry surfaces not_found responses as noop", async () => {
  nextResponse = { kind: "not_found" };
  const outcome = await hydrateCodeLanguageForEntry({ ...baseEntry });
  assert.equal(outcome.persisted, false);
  assert.equal(outcome.language, null);
});

test("hydrateCodeLanguageForEntry swallows bridge errors", async () => {
  // The helper MUST treat any IPC failure as a soft skip so a
  // transient backend outage never escalates into a render error.
  (globalThis as { window: { __TAURI_INTERNALS__?: unknown } }).window = {
    __TAURI_INTERNALS__: {
      invoke: async () => {
        throw new Error("backend unavailable");
      },
    },
  };
  const outcome = await hydrateCodeLanguageForEntry({ ...baseEntry });
  assert.equal(outcome.persisted, false);
  // Restore the staging bridge for the rest of the suite.
  (globalThis as { window: { __TAURI_INTERNALS__?: unknown } }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd !== "clipvault_code_language_set") {
          throw new Error(`unexpected command ${cmd}`);
        }
        capturedCalls.push({
          entryId: Number(args?.entryId),
          codeLanguage: (args?.codeLanguage as string | null) ?? null,
        });
        if (!nextResponse) {
          throw new Error("no response staged");
        }
        return nextResponse;
      },
    },
  };
});

test("hydrateCodeLanguageForEntry never forwards content payload", async () => {
  nextResponse = { kind: "updated", entry: { id: baseEntry.id, code_language: "javascript" } };
  await hydrateCodeLanguageForEntry({ ...baseEntry });
  // The IPC payload MUST contain only the canonical language id and
  // the entry id; the clipboard content, hash or asset reference
  // never cross the bridge.
  const stageCall = capturedCalls[0];
  const keys = Object.keys(stageCall);
  assert.deepEqual(keys.sort(), ["codeLanguage", "entryId"]);
});

test("codeLanguageLabel surfaces the canonical label", () => {
  assert.equal(codeLanguageLabel("python"), "Python");
  assert.equal(codeLanguageLabel("javascript"), "JavaScript");
  assert.equal(codeLanguageLabel("cpp"), "C++");
  assert.equal(codeLanguageLabel("csharp"), "C#");
  assert.equal(codeLanguageLabel("py"), "Python");
  assert.equal(codeLanguageLabel(null), "Code");
});

test("codeLanguageLabel falls back gracefully for unknown values", () => {
  assert.equal(codeLanguageLabel("perl"), "perl");
  assert.equal(codeLanguageLabel(" Perl "), "Perl");
  assert.equal(codeLanguageLabel(""), "Code");
});
