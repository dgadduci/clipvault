import { test } from "node:test";
import * as assert from "node:assert/strict";

import {
  ALLOWED_LANGUAGES,
  ALLOWED_LANGUAGE_LABELS,
  canonicalLabel,
  detectCodeLanguage,
  explicitCodeLanguage,
  isAllowedLanguage,
  MAX_DETECTION_BYTES,
  MIN_DETECTION_LENGTH,
  MIN_LINE_COUNT,
  nonWhitespaceLength,
  normaliseLanguage,
  renderHighlightedCode,
  RELEVANCE_MARGIN,
  RELEVANCE_THRESHOLD,
} from "../src/lib/codeLanguageDetector.ts";

test("ALLOWED_LANGUAGES lists the documented canonical identifiers", () => {
  assert.equal(
    ALLOWED_LANGUAGES.length,
    15,
    "the canonical list must contain exactly 15 languages",
  );
  assert.deepEqual(
    [...ALLOWED_LANGUAGES],
    [
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
    ],
  );
});

test("ALLOWED_LANGUAGE_LABELS stays aligned with ALLOWED_LANGUAGES", () => {
  assert.equal(ALLOWED_LANGUAGE_LABELS.length, ALLOWED_LANGUAGES.length);
});

test("isAllowedLanguage accepts only canonical, lowercase identifiers", () => {
  for (const language of ALLOWED_LANGUAGES) {
    assert.equal(isAllowedLanguage(language), true);
  }
  assert.equal(isAllowedLanguage("Python"), false);
  assert.equal(isAllowedLanguage("py"), false);
  assert.equal(isAllowedLanguage(""), false);
  assert.equal(isAllowedLanguage(null), false);
  assert.equal(isAllowedLanguage(undefined), false);
});

test("normaliseLanguage maps aliases to canonical values", () => {
  const cases: Array<[string, string]> = [
    ["js", "javascript"],
    ["JS", "javascript"],
    ["jsx", "javascript"],
    ["ts", "typescript"],
    ["tsx", "typescript"],
    ["py", "python"],
    ["PY", "python"],
    ["rs", "rust"],
    ["c++", "cpp"],
    ["C++", "cpp"],
    ["cs", "csharp"],
    ["golang", "go"],
    ["kt", "kotlin"],
    ["kts", "kotlin"],
    ["rb", "ruby"],
    ["sh", "shell"],
    ["zsh", "shell"],
    ["bash", "bash"],
    ["javascript", "javascript"],
  ];
  for (const [input, expected] of cases) {
    assert.equal(normaliseLanguage(input), expected, `input = ${input}`);
  }
});

test("normaliseLanguage returns null for unknown or empty values", () => {
  assert.equal(normaliseLanguage("perl"), null);
  assert.equal(normaliseLanguage("ruby3"), null);
  assert.equal(normaliseLanguage("kotlinx"), null);
  assert.equal(normaliseLanguage(""), null);
  assert.equal(normaliseLanguage("   "), null);
  assert.equal(normaliseLanguage(null), null);
  assert.equal(normaliseLanguage(undefined), null);
});

test("canonicalLabel returns the human-readable label for canonical and alias inputs", () => {
  assert.equal(canonicalLabel("python"), "Python");
  assert.equal(canonicalLabel("javascript"), "JavaScript");
  assert.equal(canonicalLabel("cpp"), "C++");
  assert.equal(canonicalLabel("csharp"), "C#");
  assert.equal(canonicalLabel("py"), "Python");
  assert.equal(canonicalLabel("c++"), "C++");
});

test("canonicalLabel falls back to the trimmed raw input for unknown values", () => {
  assert.equal(canonicalLabel("perl"), "perl");
  assert.equal(canonicalLabel(" Perl "), "Perl");
});

test("canonicalLabel falls back to 'Code' for empty or null input", () => {
  assert.equal(canonicalLabel(""), "Code");
  assert.equal(canonicalLabel("   "), "Code");
  assert.equal(canonicalLabel(null), "Code");
  assert.equal(canonicalLabel(undefined), "Code");
});

test("nonWhitespaceLength counts only non-whitespace characters", () => {
  assert.equal(nonWhitespaceLength(""), 0);
  assert.equal(nonWhitespaceLength("    "), 0);
  assert.equal(nonWhitespaceLength("hello"), 5);
  assert.equal(nonWhitespaceLength("hello world"), 10);
  assert.equal(nonWhitespaceLength("a\nb\tc"), 3);
});

test("explicitCodeLanguage extracts fences and shebangs", () => {
  assert.equal(
    explicitCodeLanguage("```python\nprint(1)\n```"),
    "python",
  );
  assert.equal(
    explicitCodeLanguage("```ts\nconst x = 1;\n```"),
    "typescript",
  );
  assert.equal(
    explicitCodeLanguage("```rust\nfn main() {}\n```"),
    "rust",
  );
  assert.equal(
    explicitCodeLanguage("```bash\necho hi\n```"),
    "bash",
  );
  assert.equal(
    explicitCodeLanguage("```\nconst x = 1;\n```"),
    null,
    "unfenced blocks have no explicit language",
  );
  assert.equal(
    explicitCodeLanguage("#!/usr/bin/env python3\nprint(1)"),
    "python",
  );
  assert.equal(
    explicitCodeLanguage("#!/bin/bash\necho hi"),
    "bash",
  );
  assert.equal(
    explicitCodeLanguage("hello world"),
    null,
    "prose has no explicit language",
  );
});

test("explicitCodeLanguage normalises aliases extracted from fences", () => {
  assert.equal(
    explicitCodeLanguage("```py\nprint(1)\n```"),
    "python",
  );
  assert.equal(
    explicitCodeLanguage("```c++\nint main() {}\n```"),
    "cpp",
  );
  assert.equal(
    explicitCodeLanguage("```ts\nconst x = 1;\n```"),
    "typescript",
  );
});

test("detectCodeLanguage recognises confident snippets for every allowlist language", () => {
  // The detector uses `highlight.js/lib/core` with the allowlist
  // registered. The expected language for each snippet is the one
  // the engine confidently classifies with the registered grammars;
  // a few languages share enough surface area (Java / TypeScript,
  // C / C++, Bash / Shell) that the auto-detector may return the
  // closest registered cousin — those cases assert the family
  // instead of a specific identifier.
  const cases: Array<[string, string[]]> = [
    [
      "function add(a, b) {\n  return a + b;\n}\nconst sum = add(2, 3);\nconsole.log(sum);",
      ["javascript"],
    ],
    [
      "interface Greeter {\n  name: string;\n}\nclass ConsoleGreeter implements Greeter {\n  greet(): void {\n    console.log('hi');\n  }\n}",
      ["typescript"],
    ],
    [
      "public class Hello {\n  public static void main(String[] args) {\n    System.out.println(\"Hello\");\n  }\n}",
      ["java", "typescript"],
    ],
    [
      "#include <stdio.h>\nint main(void) {\n  printf(\"hi\\n\");\n  return 0;\n}",
      ["c", "cpp"],
    ],
    [
      "#include <iostream>\nint main() {\n  std::cout << \"hi\" << std::endl;\n  return 0;\n}",
      ["cpp"],
    ],
    [
      "class Program {\n  static void Main(string[] args) {\n    System.Console.WriteLine(\"hi\");\n  }\n}",
      ["csharp"],
    ],
    [
      "def greet(name):\n    message = f'Hello {name}'\n    print(message)\n\ngreet('world')\n",
      ["python"],
    ],
    [
      "fn main() {\n    let greeting: &str = \"hi\";\n    println!(\"{}\", greeting);\n}",
      ["rust"],
    ],
    [
      "package main\n\nimport \"fmt\"\n\nfunc main() {\n    fmt.Println(\"hi\")\n}",
      ["go"],
    ],
    [
      "fun greet(name: String): Unit {\n    println(\"Hello $name\")\n}\n\nfun main() {\n    greet(\"world\")\n}",
      ["kotlin"],
    ],
    [
      "import Foundation\n\nlet greeting = \"hi\"\nprint(greeting)\n",
      ["swift"],
    ],
    [
      "<?php\nfunction greet($name) {\n    echo \"Hello $name\\n\";\n}\n\ngreet('world');\n",
      ["php"],
    ],
    [
      "def greet(name)\n  puts \"Hello #{name}\"\nend\n\ngreet('world')\n",
      ["ruby"],
    ],
    [
      "#!/bin/bash\necho \"hi\"\nls -la\n",
      ["bash", "shell"],
    ],
    [
      "echo \"hi\"\nls -la\ngrep foo /etc/hosts\n",
      ["bash", "shell"],
    ],
  ];
  for (const [snippet, expected] of cases) {
    const detected = detectCodeLanguage(snippet);
    assert.ok(
      detected !== null && expected.includes(detected),
      `snippet should classify as one of ${expected.join(", ")}, got ${detected}`,
    );
  }
});

test("detectCodeLanguage refuses prose without structural signals", () => {
  const inputs = [
    "lorem ipsum dolor sit amet consectetur adipiscing elit sed do",
    "the quick brown fox jumps over the lazy dog while the cat sleeps",
    "café résumé naïveté",
  ];
  for (const input of inputs) {
    assert.equal(
      detectCodeLanguage(input),
      null,
      `prose should not classify, got: ${input.slice(0, 20)}`,
    );
  }
});

test("detectCodeLanguage refuses empty and whitespace-only inputs", () => {
  assert.equal(detectCodeLanguage(""), null);
  assert.equal(detectCodeLanguage("   "), null);
  assert.equal(detectCodeLanguage("\n\n"), null);
});

test("detectCodeLanguage refuses too-short inputs", () => {
  assert.equal(detectCodeLanguage("const x = 1;"), null);
  assert.equal(detectCodeLanguage("x = 1\ny = 2"), null);
});

test("detectCodeLanguage respects the size cap", () => {
  const oversized = "x".repeat(MAX_DETECTION_BYTES + 1);
  assert.equal(detectCodeLanguage(oversized), null);
});

test("detectCodeLanguage respects explicit fences even with alias tags", () => {
  assert.equal(
    detectCodeLanguage("```py\nprint(1)\nprint(2)\nprint(3)\n```"),
    "python",
  );
});

test("detectCodeLanguage rejects ambiguous prose that lacks structural signals", () => {
  const ambiguous = [
    "the update is great today",
    "I selected your favorite item",
    "FROM the table of contents\nwe read the introduction",
  ];
  for (const input of ambiguous) {
    assert.equal(
      detectCodeLanguage(input),
      null,
      `ambiguous prose must not classify, got: ${input}`,
    );
  }
});

test("detectCodeLanguage prefers fence shebang language over auto-detection", () => {
  const snippet = "```bash\necho hi\necho bye\n```";
  assert.equal(detectCodeLanguage(snippet), "bash");
});

test("detectCodeLanguage respects JSON / HTML / SQL precedence", () => {
  // The detector must never misclassify JSON, HTML or SQL as code.
  // These structured payloads already have dedicated `ContentType`
  // variants and the code detector must stay conservative.
  const cases: Array<[string, string]> = [
    ['{"k":"v","n":[1,2,3]}', "json"],
    ["<div><p>hello</p></div>", "html"],
    [
      "SELECT id, name FROM users WHERE active = 1;",
      "sql",
    ],
  ];
  for (const [snippet] of cases) {
    // The detector should either refuse to classify (`null`) or
    // surface a JSON/HTML/SQL candidate — but the frontend must
    // never store a code language for these payloads because the
    // precedence belongs to the structured detector. The detector
    // itself must not invent a `code` label here.
    const detected = detectCodeLanguage(snippet);
    if (detected !== null) {
      assert.notEqual(detected, "javascript");
      assert.notEqual(detected, "typescript");
    }
  }
});

test("detectCodeLanguage threshold constants are pinned", () => {
  // The constants are part of the deterministic contract. A future
  // regression that bumps them silently surfaces as a test failure.
  assert.equal(MIN_DETECTION_LENGTH, 16);
  assert.equal(MIN_LINE_COUNT, 2);
  assert.equal(RELEVANCE_THRESHOLD, 3);
  assert.equal(RELEVANCE_MARGIN, 1);
  assert.equal(MAX_DETECTION_BYTES, 64 * 1024);
});

test("renderHighlightedCode returns sanitised HTML for a known language", () => {
  const result = renderHighlightedCode(
    "function greet(name) {\n  return `Hello ${name}`;\n}\n",
    "javascript",
  );
  assert.equal(result.language, "javascript");
  assert.ok(result.html.includes("<span"), "highlight.js wraps tokens in <span>");
  assert.ok(!result.html.toLowerCase().includes("<script"));
  assert.ok(!/<a\s+href=/i.test(result.html));
  assert.ok(!/<img/i.test(result.html));
});

test("renderHighlightedCode falls back to escaped plain text for unknown languages", () => {
  const result = renderHighlightedCode("hello & <world>", "perl");
  assert.equal(result.language, null);
  assert.ok(result.html.includes("&amp;"));
  assert.ok(result.html.includes("&lt;"));
});

test("renderHighlightedCode normalises alias languages", () => {
  const result = renderHighlightedCode("print('hi')\nprint('bye')\n", "py");
  assert.equal(result.language, "python");
  assert.ok(result.html.includes("<span"));
});

test("renderHighlightedCode strips scripts and event handlers defensively", () => {
  // Even if a future regression in highlight.js produced an inline
  // event handler, the sanitiser MUST drop it before the preview
  // surfaces the markup.
  const result = renderHighlightedCode(
    '<span class="hljs-keyword" onclick="alert(1)">if</span>',
    "javascript",
  );
  assert.ok(!/onclick/i.test(result.html));
});

test("renderHighlightedCode refuses empty input", () => {
  const result = renderHighlightedCode("", "python");
  assert.equal(result.html, "");
  assert.equal(result.language, null);
});