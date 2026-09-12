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
