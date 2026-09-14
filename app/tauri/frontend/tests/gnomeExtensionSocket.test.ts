/**
 * Regression coverage for the bundled GNOME extension transport.
 *
 * GNOME Shell 42 exposes `Gio.SocketClient.connect_async` with three
 * arguments: connectable, cancellable and callback. Passing the priority
 * placeholder used by other GLib async APIs throws before the hello frame is
 * written, leaving an otherwise enabled integration in activation_pending.
 *
 * The transport is metadata-only: the extension MUST NOT carry icon
 * bytes, window titles, PIDs, paths or `Exec=` payloads on the wire.
 * The `_resolveFocusedAppId()` helper MUST reject window-backed
 * `Shell.App` instances and the `window:*` prefix defensively while
 * preserving every non-empty Desktop File ID, including its
 * `.desktop` suffix.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();
const extensionSource = readFileSync(
  path.resolve(
    FRONTEND_ROOT,
    "..",
    "src-tauri",
    "resources",
    "gnome-extension",
    "extension.js",
  ),
  "utf8",
);

test("the GNOME extension exports the legacy lifecycle entry points", () => {
  // GNOME Shell 42 loads legacy extensions through `init()` before it
  // evaluates `enable()`. Omitting it can leave an extension reported as
  // enabled without ever starting its local bridge. It must remain a no-op:
  // consent-gated connection setup belongs exclusively to `enable()`.
  assert.match(extensionSource, /function init\(\)\s*\{\s*\}/);
  assert.match(extensionSource, /function enable\(\)/);
  assert.match(extensionSource, /function disable\(\)/);
});

test("the GNOME extension uses GNOME 42's GSocketClient constructor and async connect signatures", () => {
  assert.match(
    extensionSource,
    /new Gio\.SocketClient\(\{\s*type: Gio\.SocketType\.STREAM,\s*\}\)/,
  );
  assert.doesNotMatch(extensionSource, /socket_type:\s*Gio\.SocketType\.STREAM/);
  assert.match(
    extensionSource,
    /socket\.connect_async\(\s*addr,\s*null,\s*function \(client, result\) \{/,
  );
  assert.doesNotMatch(
    extensionSource,
    /addr,\s*null,\s*GLib\.PRIORITY_DEFAULT,\s*null,\s*function \(client, result\)/,
  );
});

test("the GNOME extension owns a Mutter Quick Paste accelerator for its enabled lifetime", () => {
  assert.match(extensionSource, /const Meta = imports\.gi\.Meta/);
  assert.match(extensionSource, /const QUICK_PASTE_ACCELERATOR = '<Control><Shift>v'/);
  assert.match(
    extensionSource,
    /global\.display\.grab_accelerator\(\s*QUICK_PASTE_ACCELERATOR,\s*Meta\.KeyBindingFlags\.IGNORE_AUTOREPEAT,/,
  );
  assert.match(
    extensionSource,
    /Main\.wm\.allowKeybinding\(bindingName, QUICK_PASTE_ACTION_MODES\)/,
  );
  assert.match(
    extensionSource,
    /global\.display\.connect\(\s*'accelerator-activated'/,
  );
  assert.match(extensionSource, /global\.display\.ungrab_accelerator\(action\)/);
  assert.match(
    extensionSource,
    /Main\.wm\.allowKeybinding\(bindingName, Shell\.ActionMode\.NONE\)/,
  );
});

test("the GNOME accelerator serialises only the metadata-free Quick Paste request", () => {
  const quickPasteEnvelope = extensionSource.match(
    /JSON\.stringify\(\s*\{\s*v:\s*PROTOCOL_VERSION,\s*kind:\s*'quick_paste',\s*\}\s*\)\s*\+\s*'\\n'/,
  );
  assert.ok(quickPasteEnvelope, "quick_paste envelope must exist");
  assert.doesNotMatch(
    quickPasteEnvelope[0],
    /\b(app_id|title|pid|path|hash|snippet)\s*:/,
    "quick_paste envelope must not carry focus or clipboard metadata",
  );
  assert.match(
    extensionSource,
    /_checkFocus\(\);\s*_publishQuickPaste\(\);/,
    "focus metadata must be queued before the Quick Paste request",
  );
  assert.match(
    extensionSource,
    /AppState\.pending_quick_paste = false;/,
    "a socket reset must discard any stale Quick Paste request",
  );
});

test("the GNOME extension refuses to publish ids for window-backed Shell.App instances", () => {
  // The extension MUST consult `is_window_backed()` when present and
  // collapse the resolved id to absence so the channel never carries
  // a value the provider cannot resolve against a `.desktop` file.
  assert.match(
    extensionSource,
    /typeof focusApp\.is_window_backed === 'function'\s*&&\s*focusApp\.is_window_backed\(\)/,
  );
});

test("the GNOME extension rejects the defensive `window:` prefix as absence", () => {
  // Some older GNOME Shell runtimes lack `is_window_backed()` or
  // throw when the focus app is being torn down. The defensive guard
  // makes any leading `window:` token collapse to absence, matching
  // the metadata-only contract documented in `design.md`.
  assert.match(
    extensionSource,
    /trimmed\.indexOf\('window:'\) === 0/,
  );
});

test("the GNOME extension preserves Desktop File IDs with the `.desktop` suffix intact", () => {
  // The Desktop Entry Specification defines the id as the basename of
  // the `.desktop` file, including the suffix. The extension MUST NOT
  // strip it: the Linux provider resolves the match by comparing the
  // full filename. Assert the body of `_resolveFocusedAppId()` does
  // not rewrite the trimmed value before returning it.
  const resolverMatch = extensionSource.match(
    /function _resolveFocusedAppId\(\)\s*\{[\s\S]*?\n\}/,
  );
  assert.ok(resolverMatch, "resolver function must exist");
  const body = resolverMatch[0];
  assert.doesNotMatch(
    body,
    /\.replace\(['"`]\.desktop['"`],/,
    "resolver must not strip the `.desktop` suffix",
  );
});

test("the GNOME extension never adds title, pid or icon fields to the IPC envelope", () => {
  // Pin the wire-protocol surface: only `{ v, kind, app_id }` is
  // allowed. The envelope MUST NOT grow `title`, `pid`, `exec`,
  // `icon`, `path` or any other identifier regardless of which
  // helpers the resolver exercises. The check scopes itself to the
  // JSON.stringify that produces the wire envelope so legitimate
  // internal property names (e.g. `socket_path`) stay untouched.
  const envelopeMatch = extensionSource.match(
    /JSON\.stringify\(\s*\{[\s\S]*?\}\s*\)\s*\+\s*'\\n'/,
  );
  assert.ok(envelopeMatch, "envelope JSON.stringify must exist");
  const envelopeLiteral = envelopeMatch[0];
  for (const forbidden of [
    "title",
    "pid",
    "exec",
    "icon",
    "path",
    "app_icon",
    "source_app",
    "desktop_id",
    "wm_class",
    "hash",
    "snippet",
  ]) {
    assert.doesNotMatch(
      envelopeLiteral,
      new RegExp(`\\b${forbidden}\\s*:`),
      `envelope must not carry a "${forbidden}" field`,
    );
  }
  assert.match(
    envelopeLiteral,
    /\bv\s*:\s*PROTOCOL_VERSION\s*,\s*kind\s*:\s*'app_id'\s*,\s*app_id\s*:/,
    "envelope must serialise exactly { v, kind, app_id }",
  );
  assert.doesNotMatch(
    extensionSource,
    /focusApp\.get_icon\(/,
    "resolver must not consult Shell.App.get_icon()",
  );
  assert.doesNotMatch(
    extensionSource,
    /Meta\.Window\.get_title/,
    "resolver must not consult window titles",
  );
  assert.doesNotMatch(
    extensionSource,
    /get_wm_class/,
    "resolver must not infer the app from WM_CLASS",
  );
  assert.doesNotMatch(
    extensionSource,
    /get_pid/,
    "resolver must not transport the PID",
  );
  assert.doesNotMatch(
    extensionSource,
    /get_gtk_application_id/,
    "resolver must not transport the GTK application id",
  );
});
