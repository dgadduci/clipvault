/**
 * Coalescing / deduplication helper the Desktop rail and Quick
 * Paste use to drive the `clipvault_code_language_set` bridge.
 *
 * The helper guarantees:
 *
 *   - the detector runs **at most once per `entry_id` per surface**;
 *     a remount, a hot-reload or a stale `history-updated` event
 *     cannot multiply the IPC calls;
 *   - only the freshest detection result wins; an earlier request
 *     whose result lands after a newer capture (or a duplicate
 *     capture that changed the entry id) is silently dropped so
 *     the rail never flashes a stale language;
 *   - the IPC payload is metadata-only: the helper never echoes the
 *     clipboard content, the source-app identifier, the content
 *     hash or the asset reference back to the Tauri command;
 *   - the helper refuses to call the bridge for already-classified
 *     rows (an entry with a non-null `code_language` is left
 *     untouched) so the persistence layer's "no overwrite with
 *     null" rule stays a no-op the service can absorb.
 */

import { codeLanguageSetCommand } from "./tauri.ts";
import {
  ALLOWED_LANGUAGES,
  canonicalLabel,
  detectCodeLanguage,
  normaliseLanguage,
} from "./codeLanguageDetector.ts";

export interface HydratableEntryLike {
  id: number;
  content: string;
  content_type: string;
  code_language: string | null;
}

export interface CodeLanguageHydrationOutcome {
  /** The classification the helper ultimately persisted, or `null`. */
  language: string | null;
  /**
   * `true` when the helper decided to call the bridge and the call
   * resolved successfully. `false` when the helper skipped the
   * call (already classified, no candidate, payload too large)
   * or when the bridge returned `noop` / `not_found`.
   */
  persisted: boolean;
}

/**
 * In-flight tracker per `entry_id`. The Map is process-local: each
 * `App.svelte` and `QuickPaste.svelte` mount owns its own
 * registry so the two surfaces cannot race each other through the
 * same Tauri command.
 *
 * The entry carries both a monotonic token (used to drop stale
 * responses) and the in-flight `Promise` (used to coalesce
 * concurrent requests into a single IPC round-trip).
 */
interface PendingClassification {
  token: number;
  language: string | null;
  /** Shared in-flight promise so concurrent requests share one IPC. */
  inflight: Promise<unknown> | null;
}

const pendingByEntry = new Map<number, PendingClassification>();

/**
 * Reset the in-flight tracker. Tests use it to guarantee isolation
 * between scenarios; production code never calls it.
 */
export function __resetCodeLanguageHydrationForTests(): void {
  pendingByEntry.clear();
}

/**
 * Run the detector over `entry.content`, persist the result when
 * the helper accepts it and return the metadata-only outcome the
 * caller can apply to its local state.
 *
 * `options.kindHint` lets callers (the capture pipeline, the
 * `history-updated` event handler) skip the detector when the
 * backend already classified the entry — typically `code` for a
 * fenced capture or `text` for ambiguous prose. The detector still
 * owns the decision; the hint only short-circuits work the caller
 * already knows to skip.
 */
export async function hydrateCodeLanguageForEntry(
  entry: HydratableEntryLike,
  options: {
    kindHint?: "code" | "text";
    detectionOptions?: {
      minLength?: number;
      minLineCount?: number;
      relevanceThreshold?: number;
      relevanceMargin?: number;
      maxBytes?: number;
    };
  } = {},
): Promise<CodeLanguageHydrationOutcome> {
  const existing = pendingByEntry.get(entry.id);
  const token = (existing?.token ?? 0) + 1;
  const placeholder: PendingClassification = {
    token,
    language: null,
    inflight: existing?.inflight ?? null,
  };
  pendingByEntry.set(entry.id, placeholder);
  if (entry.code_language !== null && entry.code_language.length > 0) {
    pendingByEntry.set(entry.id, {
      token,
      language: entry.code_language,
      inflight: existing?.inflight ?? null,
    });
    return { language: entry.code_language, persisted: false };
  }
  if (options.kindHint === "text") {
    pendingByEntry.set(entry.id, {
      token,
      language: null,
      inflight: existing?.inflight ?? null,
    });
    return { language: null, persisted: false };
  }
  if (entry.content_type !== "code" && entry.content_type !== "text") {
    pendingByEntry.set(entry.id, {
      token,
      language: null,
      inflight: existing?.inflight ?? null,
    });
    return { language: null, persisted: false };
  }
  const detected = detectCodeLanguage(entry.content, options.detectionOptions);
  pendingByEntry.set(entry.id, {
    token,
    language: detected,
    inflight: existing?.inflight ?? null,
  });
  if (detected === null) {
    return { language: null, persisted: false };
  }
  // Coalesce concurrent requests: if a previous call for the same
  // entry is still in flight, await its promise instead of issuing
  // a second bridge call. The freshest token still wins on the
  // shared response.
  if (existing?.inflight && existing.token === token - 1) {
    try {
      await existing.inflight;
    } catch {
      // fall through to the local call below
    }
    const after = pendingByEntry.get(entry.id);
    if (!after || after.token !== token) {
      return { language: detected, persisted: false };
    }
    return {
      language: after.language,
      persisted: false,
    };
  }
  const bridgeCall = codeLanguageSetCommand({
    entryId: entry.id,
    codeLanguage: detected,
  });
  placeholder.inflight = bridgeCall;
  pendingByEntry.set(entry.id, placeholder);
  try {
    const response = await bridgeCall;
    const freshest = pendingByEntry.get(entry.id);
    if (!freshest || freshest.token !== token) {
      return { language: detected, persisted: false };
    }
    if (response.kind === "updated") {
      const stored = response.entry.code_language;
      if (stored !== null) {
        pendingByEntry.set(entry.id, {
          token,
          language: stored,
          inflight: null,
        });
        return { language: stored, persisted: true };
      }
      pendingByEntry.set(entry.id, {
        token,
        language: detected,
        inflight: null,
      });
      return { language: detected, persisted: false };
    }
    if (response.kind === "noop") {
      const stored = response.entry.code_language;
      pendingByEntry.set(entry.id, {
        token,
        language: stored,
        inflight: null,
      });
      return { language: stored, persisted: false };
    }
    pendingByEntry.set(entry.id, {
      token,
      language: null,
      inflight: null,
    });
    return { language: null, persisted: false };
  } catch {
    pendingByEntry.set(entry.id, {
      token,
      language: detected,
      inflight: null,
    });
    return { language: detected, persisted: false };
  }
}

/**
 * Build the human-readable label the card / preview surfaces for a
 * given `code_language`. The helper mirrors `canonicalLabel` so
 * every UI surface falls back to the same default copy.
 */
export function codeLanguageLabel(language: string | null): string {
  return canonicalLabel(language);
}

/** Re-export the allowlist so card code does not import the
 * detector directly when it only needs the canonical list. */
export { ALLOWED_LANGUAGES };

/** Re-export the normaliser so tests can verify alias handling. */
export { normaliseLanguage };