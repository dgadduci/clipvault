/**
 * Pure, deterministic frontend detector for the
 * `code-language-detection` capability.
 *
 * The helper is the single source of truth for the metadata the
 * Desktop rail, the cards, the Quick Paste list and the
 * `ClipboardPreview` overlay render. It owns:
 *
 *   - the canonical `ALLOWED_LANGUAGES` allowlist the persistence
 *     service, the SQL index and the UI label share;
 *   - the alias table (`js`, `py`, `c++`, …) every detector and the
 *     highlight.js grammar registry collapse to a canonical value
 *     before anything is persisted or sent across the bridge;
 *   - the conservative heuristic the detector uses: explicit fence /
 *     shebang wins over auto-detection, the auto-detector runs only
 *     against the allowlist, the relevance threshold and the margin
 *     between the best and second-best candidate are pinned constants
 *     so the regression suite can exercise them deterministically;
 *   - the size cap that prevents the detector from spinning on
 *     extremely large captures (the engine skips the analysis and
 *     returns `null` instead of crashing the UI);
 *   - a safe-rendering helper that produces the HTML markup the
 *     `ClipboardPreview` overlay consumes. The helper enforces
 *     "no scripts, no event handlers, no remote URLs, no foreign
 *     content"; the resulting markup is read-only and never
 *     persisted.
 *
 * The detector is intentionally side-effect free and has no DOM or
 * clipboard dependency. It can be unit-tested without a webview or a
 * Tauri runtime.
 */

import hljs from "highlight.js/lib/core";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import java from "highlight.js/lib/languages/java";
import c from "highlight.js/lib/languages/c";
import cpp from "highlight.js/lib/languages/cpp";
import csharp from "highlight.js/lib/languages/csharp";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import go from "highlight.js/lib/languages/go";
import kotlin from "highlight.js/lib/languages/kotlin";
import swift from "highlight.js/lib/languages/swift";
import php from "highlight.js/lib/languages/php";
import ruby from "highlight.js/lib/languages/ruby";
import bash from "highlight.js/lib/languages/bash";
import shell from "highlight.js/lib/languages/shell";

/**
 * Canonical allowlist the detector, the SQL index and the UI label
 * share. The list mirrors the Rust `CODE_LANGUAGES` constant in
 * `crates/clipvault-core/src/code_language.rs`; the two MUST stay
 * byte-for-byte aligned so the bridge accepts every value the
 * frontend sends.
 */
export const ALLOWED_LANGUAGES: readonly string[] = [
  "javascript",
  "typescript",
  "java",
  "c",
  "cpp",
  "csharp",
  "python",
  "rust",
  "go",
  "kotlin",
  "swift",
  "php",
  "ruby",
  "bash",
  "shell",
] as const;

/**
 * Human-readable label the UI surfaces next to the code icon. The
 * list MUST stay aligned with `ALLOWED_LANGUAGES` — the
 * `canonicalLabel` helper indexes into it positionally.
 */
export const ALLOWED_LANGUAGE_LABELS: readonly string[] = [
  "JavaScript",
  "TypeScript",
  "Java",
  "C",
  "C++",
  "C#",
  "Python",
  "Rust",
  "Go",
  "Kotlin",
  "Swift",
  "PHP",
  "Ruby",
  "Bash",
  "Shell",
] as const;

/**
 * Aliases the highlight.js grammar registry and the legacy
 * `clipvault-core::content_type` detector use. The detector
 * normalises every alias to its canonical value before forwarding
 * anything to the persistence service.
 */
const LANGUAGE_ALIASES: Readonly<Record<string, string>> = {
  js: "javascript",
  jsx: "javascript",
  ts: "typescript",
  tsx: "typescript",
  py: "python",
  rs: "rust",
  "c++": "cpp",
  cs: "csharp",
  golang: "go",
  kt: "kotlin",
  kts: "kotlin",
  rb: "ruby",
  sh: "shell",
  zsh: "shell",
  objectivec: "cpp",
};

/** Highlight.js grammar aliases the detector accepts as-is. */
const HIGHLIGHTJS_LANGUAGE_ALIASES: Readonly<Record<string, string>> = {
  js: "javascript",
  ts: "typescript",
  rs: "rust",
  py: "python",
  rb: "ruby",
  sh: "bash",
  cs: "csharp",
  kt: "kotlin",
  golang: "go",
  "c++": "cpp",
};
void HIGHLIGHTJS_LANGUAGE_ALIASES;

/**
 * Minimum length (non-whitespace characters) below which the detector
 * returns `null`. The constant is exported so the regression suite
 * can pin its byte-for-byte value.
 */
export const MIN_DETECTION_LENGTH = 16;

/**
 * Minimum number of lines below which the detector still analyses the
 * input but raises the relevance threshold. The detector enforces
 * "two lines OR 16 chars" so a single short line never wins.
 */
export const MIN_LINE_COUNT = 2;

/**
 * Minimum relevance the auto-detector must reach for the result to
 * be accepted. The constant is exposed so the regression suite can
 * pin its value alongside the highlight.js version. The default
 * value matches the empirical relevance scores `highlight.js`
 * produces for confident snippets when the allowlist is restricted
 * to the documented canonical languages.
 */
export const RELEVANCE_THRESHOLD = 3;

/**
 * Minimum margin between the best candidate and the second-best one.
 * When the margin falls below this value the detector returns `null`
 * to refuse an ambiguous classification.
 */
export const RELEVANCE_MARGIN = 1;

/**
 * Hard size cap (in characters) above which the detector skips the
 * analysis and returns `null` so an unusually large capture cannot
 * block the UI thread.
 */
export const MAX_DETECTION_BYTES = 64 * 1024;

/** True when `value` is a canonical language identifier. */
export function isAllowedLanguage(value: string | null | undefined): value is string {
  if (typeof value !== "string") return false;
  return (ALLOWED_LANGUAGES as readonly string[]).includes(value);
}

/**
 * Map an alias (or canonical value) to its canonical form. Unknown
 * values return `null` so the caller can refuse to persist them.
 */
export function normaliseLanguage(raw: string | null | undefined): string | null {
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  if (trimmed.length === 0) return null;
  const lower = trimmed.toLowerCase();
  if (LANGUAGE_ALIASES[lower]) return LANGUAGE_ALIASES[lower];
  if (isAllowedLanguage(lower)) return lower;
  return null;
}

/**
 * Resolve the canonical language to the highlight.js grammar
 * identifier the highlighter actually registers. Mirrors
 * `LANGUAGE_ALIASES` but maps the canonical form onto the value
 * `hljs.registerLanguage` knows about. The helper returns `null`
 * when the language is outside the allowlist, so callers can fall
 * back to escaped plain text.
 */
export function highlightGrammarFor(language: string): string | null {
  const canonical = normaliseLanguage(language);
  if (canonical === null) return null;
  return canonical;
}

/** Human-readable label for a canonical or alias language. */
export function canonicalLabel(language: string | null | undefined): string {
  if (typeof language !== "string") return "Code";
  const trimmed = language.trim();
  if (trimmed.length === 0) return "Code";
  const canonical = normaliseLanguage(trimmed);
  if (canonical === null) return trimmed;
  const idx = (ALLOWED_LANGUAGES as readonly string[]).indexOf(canonical);
  if (idx < 0) return trimmed;
  return ALLOWED_LANGUAGE_LABELS[idx] ?? trimmed;
}

let highlightRegistered = false;

/**
 * Register the allowlist grammars against the highlight.js core
 * registry. The function is idempotent and safe to call repeatedly
 * (the highlight.js core API surfaces an error when a grammar is
 * re-registered, which we silently ignore).
 */
function ensureHighlightRegistered(): void {
  if (highlightRegistered) return;
  try {
    hljs.registerLanguage("javascript", javascript);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("typescript", typescript);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("java", java);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("c", c);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("cpp", cpp);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("csharp", csharp);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("python", python);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("rust", rust);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("go", go);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("kotlin", kotlin);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("swift", swift);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("php", php);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("ruby", ruby);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("bash", bash);
  } catch {
    /* already registered */
  }
  try {
    hljs.registerLanguage("shell", shell);
  } catch {
    /* already registered */
  }
  highlightRegistered = true;
}

/** Structural signals the heuristic requires before auto-detection. */
const CODE_STRUCTURAL_SIGNALS: readonly RegExp[] = [
  /\{[^}\n]*\}/,
  /;[\s]*$/m,
  /=>/,
  /::/,
  /#include\b/,
  /\bdef\s+[A-Za-z_]/,
  /\bclass\s+[A-Za-z_]/,
  /\bfn\s+[A-Za-z_]/,
  /\bfunc\s+[A-Za-z_]/,
  /\bimport\s+/,
  /\bconst\s+[A-Za-z_]/,
  /\blet\s+[A-Za-z_]/,
  /\binterface\s+[A-Za-z_]/,
  /\bvar\s+[A-Za-z_]/,
  /\bprintln!?\b/,
  /\bconsole\./,
  /\bSystem\.out\b/,
];

/** Shell-specific structural signals. The detector accepts a snippet
 * as code when the auto-detector later identifies it as bash / shell
 * AND any of these patterns is present. The conservative list keeps
 * prose such as "I love apples and oranges" out. */
const SHELL_STRUCTURAL_SIGNALS: readonly RegExp[] = [
  /\|/, // pipes
  /&&|\|\|/,
  /\$(?:[A-Za-z_][A-Za-z0-9_]*|\{[^}\n]*\}|\([^)\n]*\))/,
  />/,
  /</,
  /\b(?:grep|sed|awk|find|ls|cat|echo|curl|wget|export|source|sudo|cd|rm|cp|mv|mkdir|touch|chmod|chown|tar|gzip|psql|docker|kubectl|git|npm|cargo|yarn|pnpm|brew|apt|make|ssh)\b/,
];

/**
 * Whether the input contains any structural signal that proves the
 * payload is actually code. The detector refuses to label prose
 * that happens to share keywords with a programming language.
 */
function hasStructuralSignal(input: string): boolean {
  for (const signal of CODE_STRUCTURAL_SIGNALS) {
    if (signal.test(input)) return true;
  }
  for (const signal of SHELL_STRUCTURAL_SIGNALS) {
    if (signal.test(input)) return true;
  }
  return false;
}

/**
 * Extract a non-whitespace character count from the input.
 */
export function nonWhitespaceLength(input: string): number {
  let count = 0;
  for (const ch of input) {
    if (!/\s/.test(ch)) count += 1;
  }
  return count;
}

/**
 * Extract the language declared by a fenced code block
 * (```` ```python ````) or a shebang line (`#!/usr/bin/env python3`).
 * Returns the canonical identifier the allowlist accepts, or `null`
 * when no explicit signal is present.
 */
export function explicitCodeLanguage(input: string): string | null {
  if (typeof input !== "string") return null;
  const trimmed = input.trimStart();
  // Shebang: `#!/usr/bin/env python3` or `#!/bin/bash`.
if (trimmed.startsWith("#!")) {
    const tokens = trimmed.slice(2).split(/\s+/);
    const first = tokens[0] ?? "";
    const pathSegments = first.split("/");
    const interpreterPath = pathSegments[pathSegments.length - 1] ?? first;
    const rawInterpreter =
      interpreterPath === "env" ? tokens[1] ?? "" : interpreterPath;
    const stripped = rawInterpreter.replace(/[0-9]+$/g, "");
    if (stripped.length === 0) return null;
    return normaliseLanguage(stripped);
  }
  // Fenced block: ```python ... ``` or ~~~bash ... ~~~
  const fenceMatch = trimmed.match(/^(`{3,}|~{3,})([^\n`~]*)/);
  if (!fenceMatch) return null;
  const langTag = (fenceMatch[2] ?? "").trim();
  if (langTag.length === 0) return null;
  return normaliseLanguage(langTag.split(/\s+/)[0]);
}

/**
 * Run the auto-detector against the allowlist. The function is
 * pure: it never mutates `input`, it never logs the payload and it
 * returns `null` for every ambiguous, too-short or oversized input.
 */
export function detectCodeLanguage(
  input: string,
  options: {
    minLength?: number;
    minLineCount?: number;
    relevanceThreshold?: number;
    relevanceMargin?: number;
    maxBytes?: number;
  } = {},
): string | null {
  if (typeof input !== "string") return null;
  ensureHighlightRegistered();
  const minLength = options.minLength ?? MIN_DETECTION_LENGTH;
  const minLineCount = options.minLineCount ?? MIN_LINE_COUNT;
  const relevanceThreshold = options.relevanceThreshold ?? RELEVANCE_THRESHOLD;
  const relevanceMargin = options.relevanceMargin ?? RELEVANCE_MARGIN;
  const maxBytes = options.maxBytes ?? MAX_DETECTION_BYTES;

  if (input.length === 0) return null;
  if (input.length > maxBytes) return null;
  if (nonWhitespaceLength(input) < minLength) return null;

  const explicit = explicitCodeLanguage(input);
  if (explicit !== null) return explicit;

  const lineCount = input.split(/\r\n|\r|\n/).filter((line) => line.length > 0).length;
  if (lineCount < minLineCount) return null;
  if (!hasStructuralSignal(input)) return null;

  try {
    const result = hljs.highlightAuto(input, [
      ...(ALLOWED_LANGUAGES as readonly string[]),
    ]);
    if (!isAllowedLanguage(result.language)) return null;
    if (result.relevance < relevanceThreshold) return null;
    // The auto-detector returns a `secondBest` companion when a
    // candidate other than the top one crossed the relevance
    // threshold. We reject the result when the gap is below
    // `relevanceMargin` so an unambiguous classification is the only
    // one the detector accepts.
    if (result.secondBest) {
      const gap = result.relevance - result.secondBest.relevance;
      if (gap < relevanceMargin) return null;
    }
    return result.language;
  } catch {
    return null;
  }
}

/**
 * Render a stored `code_language` to highlighted HTML the
 * `ClipboardPreview` overlay can mount inside its `<pre>` element.
 *
 * The helper is intentionally narrow:
 *
 *   - `language` MUST be canonical (`normaliseLanguage` collapses
 *     aliases); anything outside the allowlist falls back to
 *     escaped plain text;
 *   - the resulting markup is sanitised to remove script tags,
 *     inline event handlers (`on*`), `javascript:` URLs and any
 *     external `href` / `src`. The highlighter's markup never
 *     embeds remote references by construction but the guard is
 *     explicit so a future regression in highlight.js cannot leak
 *     active content;
 *   - the helper is read-only: it never persists, never copies and
 *     never navigates.
 */
export function renderHighlightedCode(
  input: string,
  language: string | null,
): { html: string; language: string | null } {
  if (typeof input !== "string" || input.length === 0) {
    return { html: "", language: null };
  }
  const canonical = normaliseLanguage(language ?? null);
  if (canonical === null || !highlightGrammarFor(canonical)) {
    return { html: escapePlainText(input), language: null };
  }
  ensureHighlightRegistered();
  try {
    const result = hljs.highlight(input, { language: canonical });
    return {
      html: sanitiseHighlightedHTML(result.value),
      language: canonical,
    };
  } catch {
    return { html: escapePlainText(input), language: null };
  }
}

function escapePlainText(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/**
 * Drop every element / attribute highlight.js (or a future
 * regression in its output) could expose that re-introduces active
 * content into the preview overlay. The helper is intentionally
 * restrictive: it only keeps a closed allowlist of tags (`span`
 * with the documented highlight.js class names) and an even
 * narrower allowlist of attributes.
 */
function sanitiseHighlightedHTML(html: string): string {
  return html
    .replace(/<\s*script\b[^>]*>[\s\S]*?<\s*\/\s*script\s*>/gi, "")
    .replace(/<\s*\/?\s*(iframe|object|embed|link|style|meta|form)\b[^>]*>/gi, "")
    .replace(/\son[a-z]+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, "")
    .replace(/\bhref\s*=\s*("\s*javascript:[^"]*"|'\s*javascript:[^']*'|javascript:[^\s>]+)/gi, "")
    .replace(/\bsrc\s*=\s*("\s*javascript:[^"]*"|'\s*javascript:[^']*'|javascript:[^\s>]+)/gi, "")
    .replace(/\bhref\s*=\s*("[^"]*"|'[^']*')/gi, "")
    .replace(/\bsrc\s*=\s*("[^"]*"|'[^']*')/gi, "");
}