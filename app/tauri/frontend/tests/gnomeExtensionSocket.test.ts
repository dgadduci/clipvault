/**
 * Regression coverage for the bundled GNOME extension transport.
 *
 * GNOME Shell 42 exposes `Gio.SocketClient.connect_async` with three
 * arguments: connectable, cancellable and callback. Passing the priority
 * placeholder used by other GLib async APIs throws before the hello frame is
 * written, leaving an otherwise enabled integration in activation_pending.
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
