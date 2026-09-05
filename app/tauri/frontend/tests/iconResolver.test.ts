/**
 * Unit tests for the safe icon resolver used by `SettingsPanel`.
 *
 * The resolver is the single bridge between the opaque `icon_ref`
 * the database persists and the bytes the webview can render. The
 * tests below pin the contract:
 *
 * - `icon_ref === null` short-circuits to the fallback copy without
 *   ever touching the loader;
 * - a loader that rejects falls back to the same fallback without
 *   surfacing the underlying error;
 * - a successful load returns a `blob:` URL backed by a `Blob` with
 *   the `image/png` MIME type so the webview can decode it;
 * - `releaseFor` reclaims the URL of a single entry;
 * - `release` reclaims every URL the resolver holds and clears the
 *   cache so a subsequent `resolve` re-loads the icon;
 * - the resolver is testable without DOM globals: a minimal
 *   `URL.createObjectURL` / `URL.revokeObjectURL` shim is installed
 *   at the top of the file.
 *
 * The tests deliberately avoid any Tauri dependency; the
 * `ignoredAppIconCommand` wrapper has its own dedicated bridge
 * tests in `iconBridge.test.ts`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  createIconResolver,
  resolveIconUrl,
  type IconLoader,
} from "../src/lib/iconResolver.ts";

interface FakeUrlHub {
  created: string[];
  revoked: string[];
}

function installUrlShim(hub: FakeUrlHub): void {
  const globalScope = globalThis as unknown as {
    URL: {
      createObjectURL: (blob: Blob) => string;
      revokeObjectURL: (url: string) => void;
    };
  };
  globalScope.URL = {
    createObjectURL(blob: Blob): string {
      const url = `blob:test-${hub.created.length}-${blob.size}`;
      hub.created.push(url);
      return url;
    },
    revokeObjectURL(url: string): void {
      hub.revoked.push(url);
    },
  };
}

function bytesOf(payload: string): number[] {
  return Array.from(new TextEncoder().encode(payload));
}

test("resolveIconUrl returns null when ref is null", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const loader: IconLoader = {
    async loadIconBytes() {
      throw new Error("must not call loader");
    },
  };
  const resolution = await resolveIconUrl(null, loader);
  assert.equal(resolution.ok, false);
  assert.equal(resolution.url, null);
  assert.equal(resolution.blob, null);
  assert.equal(hub.created.length, 0);
});

test("resolveIconUrl returns null when the loader rejects", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const loader: IconLoader = {
    async loadIconBytes() {
      throw new Error("backend exploded");
    },
  };
  const resolution = await resolveIconUrl(
    "ignored-apps/com.apple.textedit.png",
    loader,
  );
  assert.equal(resolution.ok, false);
  assert.equal(resolution.url, null);
  assert.equal(hub.created.length, 0);
});

test("resolveIconUrl returns null when the loader returns null", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const loader: IconLoader = {
    async loadIconBytes() {
      return null;
    },
  };
  const resolution = await resolveIconUrl(
    "ignored-apps/com.apple.textedit.png",
    loader,
  );
  assert.equal(resolution.ok, false);
  assert.equal(hub.created.length, 0);
});

test("resolveIconUrl turns backend bytes into a png blob url", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const payload = bytesOf("fake-png-bytes");
  const loader: IconLoader = {
    async loadIconBytes() {
      return payload;
    },
  };
  const resolution = await resolveIconUrl(
    "ignored-apps/com.apple.textedit.png",
    loader,
  );
  assert.equal(resolution.ok, true);
  assert.equal(typeof resolution.url, "string");
  assert.match(resolution.url!, /^blob:test-/);
  assert.ok(resolution.blob instanceof Blob);
  assert.equal(resolution.blob!.type, "image/png");
  assert.equal(resolution.blob!.size, payload.length);
});

test("resolveIconUrl accepts Uint8Array payloads from the bridge", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const loader: IconLoader = {
    async loadIconBytes() {
      return new Uint8Array([1, 2, 3, 4, 5]);
    },
  };
  const resolution = await resolveIconUrl(
    "ignored-apps/com.apple.textedit.png",
    loader,
  );
  assert.equal(resolution.ok, true);
  assert.ok(resolution.blob instanceof Blob);
  assert.equal(resolution.blob!.type, "image/png");
  assert.equal(resolution.blob!.size, 5);
});

test("createIconResolver caches blob urls and reuses them on repeat calls", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  let calls = 0;
  const loader: IconLoader = {
    async loadIconBytes() {
      calls += 1;
      return bytesOf("fake");
    },
  };
  const resolver = createIconResolver(loader);
  const first = await resolver.resolve("ignored-apps/com.apple.textedit.png");
  const second = await resolver.resolve("ignored-apps/com.apple.textedit.png");
  assert.equal(first.ok, true);
  assert.equal(second.ok, true);
  assert.equal(first.url, second.url, "second resolve must reuse the cached url");
  assert.equal(calls, 1, "loader must run exactly once for repeated resolve calls");
  assert.equal(hub.created.length, 1);
  assert.equal(resolver.cacheSize(), 1);
});

test("createIconResolver.release reclaims every url", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const loader: IconLoader = {
    async loadIconBytes() {
      return bytesOf("payload");
    },
  };
  const resolver = createIconResolver(loader);
  await resolver.resolve("ignored-apps/a.png");
  await resolver.resolve("ignored-apps/b.png");
  assert.equal(resolver.cacheSize(), 2);
  assert.equal(hub.created.length, 2);
  resolver.release();
  assert.equal(resolver.cacheSize(), 0);
  assert.equal(hub.revoked.length, 2);
});

test("createIconResolver.releaseFor only reclaims the matching entry", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const loader: IconLoader = {
    async loadIconBytes() {
      return bytesOf("payload");
    },
  };
  const resolver = createIconResolver(loader);
  const a = await resolver.resolve("ignored-apps/a.png");
  const b = await resolver.resolve("ignored-apps/b.png");
  assert.equal(resolver.cacheSize(), 2);
  resolver.releaseFor("ignored-apps/a.png");
  assert.equal(resolver.cacheSize(), 1);
  assert.deepEqual(hub.revoked, [a.url]);
  assert.equal(b.url!.startsWith("blob:"), true);
});

test("createIconResolver stays usable after a release and reloads the blob", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  let calls = 0;
  const loader: IconLoader = {
    async loadIconBytes() {
      calls += 1;
      return bytesOf("payload");
    },
  };
  const resolver = createIconResolver(loader);
  await resolver.resolve("ignored-apps/a.png");
  resolver.release();
  assert.equal(resolver.cacheSize(), 0);
  const after = await resolver.resolve("ignored-apps/a.png");
  assert.equal(after.ok, true);
  assert.equal(calls, 2, "release must drop the cached loader result");
  assert.equal(hub.created.length, 2);
});

test("createIconResolver refuses to resolve when ref is null without calling the loader", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  let called = false;
  const loader: IconLoader = {
    async loadIconBytes() {
      called = true;
      return null;
    },
  };
  const resolver = createIconResolver(loader);
  const result = await resolver.resolve(null);
  assert.equal(result.ok, false);
  assert.equal(called, false);
  assert.equal(resolver.cacheSize(), 0);
});

test("createIconResolver distinguishes delivered bytes from rejected refs", async () => {
  // The settings panel keeps two separate signals: `iconUrls[id]`
  // (the resolver returned a blob URL because the backend delivered
  // bytes) and `iconFailures[id]` (the backend rejected the ref, so
  // the bytes never arrived). The resolver must surface the second
  // case as `ok: false` so the panel can apply the muted fallback
  // instead of the bright initial-letter render. A successful
  // resolution caches the URL exactly once and reports `ok: true`
  // for every subsequent call without re-invoking the loader.
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  const seenRefs: string[] = [];
  const loader: IconLoader = {
    async loadIconBytes(ref) {
      seenRefs.push(ref);
      if (ref.endsWith("bad.png")) {
        return null;
      }
      return bytesOf(`payload-for-${ref}`);
    },
  };
  const resolver = createIconResolver(loader);

  const good = await resolver.resolve("ignored-apps/good.png");
  assert.equal(good.ok, true, "backend delivered bytes for good.png");
  assert.ok(good.url);
  assert.equal(resolver.cacheSize(), 1, "delivered bytes must populate the cache");

  const bad = await resolver.resolve("ignored-apps/bad.png");
  assert.equal(bad.ok, false, "backend rejected bad.png — no URL must leak");
  assert.equal(bad.url, null);
  assert.equal(bad.blob, null);
  assert.equal(
    resolver.cacheSize(),
    1,
    "failed resolve must NOT pollute the cache with a stale entry",
  );
  assert.deepEqual(seenRefs, [
    "ignored-apps/good.png",
    "ignored-apps/bad.png",
  ]);

  // A repeat call for the cached good ref MUST reuse the URL without
  // touching the loader. This is the property the panel relies on to
  // skip already-resolved rows on every re-render.
  const goodAgain = await resolver.resolve("ignored-apps/good.png");
  assert.equal(goodAgain.ok, true);
  assert.equal(goodAgain.url, good.url, "cached URL must be returned");
  assert.equal(resolver.cacheSize(), 1);
  assert.deepEqual(
    seenRefs,
    ["ignored-apps/good.png", "ignored-apps/bad.png"],
    "cached good ref must not re-invoke the loader",
  );
});

test("resolveIconUrl propagates a loader factory error without leaking partial state", async () => {
  const hub: FakeUrlHub = { created: [], revoked: [] };
  installUrlShim(hub);
  let factories = 0;
  const resolver = createIconResolver(
    {
      async loadIconBytes() {
        throw new Error("backend exploded");
      },
    },
    {
      loaderFactory: () => {
        factories += 1;
        return {
          async loadIconBytes() {
            throw new Error("backend exploded");
          },
        };
      },
    },
  );
  const result = await resolver.resolve("ignored-apps/com.apple.textedit.png");
  assert.equal(result.ok, false);
  assert.equal(result.url, null);
  assert.equal(result.blob, null);
  assert.equal(factories, 1, "factory must be invoked exactly once per resolve");
  assert.equal(hub.created.length, 0);
  assert.equal(resolver.cacheSize(), 0);
});