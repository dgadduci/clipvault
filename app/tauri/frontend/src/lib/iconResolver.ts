/**
 * Safe resolver for application-icon assets.
 *
 * The settings panel renders one row per blacklisted application.
 * Every row carries an opaque `icon_ref` produced by the platform
 * application picker; the resolver turns that reference into a
 * webview-loadable URL (a `Blob:` URL pointing at the PNG bytes the
 * backend returned) while preserving the privacy and safety
 * guarantees that the rest of the app depends on:
 *
 * - `icon_ref === null` is the canonical "no icon" signal and
 *   produces `null` from the resolver; the caller falls back to the
 *   letter render.
 * - The loader is injected so unit tests can substitute a fake that
 *   returns bytes or rejects deterministically. The production loader
 *   routes through `ignoredAppIconCommand` which itself goes through
 *   the backend validator (relative path, no `..`, must start with
 *   `ignored-apps/`, must be a valid PNG). The resolver never widens
 *   that surface.
 * - Failures (loader rejects, file missing, payload too large, ...)
 *   collapse to `null` so the settings panel always renders a row,
 *   even when the icon cannot be loaded.
 * - Successful resolutions allocate a fresh `Blob` and a `blob:` URL
 *   the caller can `revoke` once the corresponding row goes away.
 *
 * The resolver never inspects the PNG bytes, never copies clipboard
 * content and never logs the underlying reference. The
 * `loader.loadIconBytes` contract keeps every byte the backend sent
 * confined to the blob that the resolver returns.
 */

export interface IconLoader {
  /**
   * Resolve `ref` to the raw PNG bytes the backend stored.
   * Implementations MUST reject with a normal `Error` (or any
   * throwable) when the reference is invalid or the file is missing;
   * the resolver converts those into a `null` URL.
   */
  loadIconBytes(ref: string): Promise<number[] | Uint8Array | null>;
}

export interface IconResolution {
  /** Whether the resolver produced a blob URL for `ref`. */
  ok: boolean;
  /** A `blob:` URL the webview can use as `<img src=...>`. `null` when the resolver fell back to the letter render. */
  url: string | null;
  /** The `Blob` backing `url`, exposed so callers can revoke the URL once the row disappears. */
  blob: Blob | null;
}

const PNG_MIME = "image/png";

/**
 * Resolve `icon_ref` to a `blob:` URL the settings panel can render.
 *
 * The function intentionally returns the resolved `Blob` alongside
 * the URL so the caller can call `URL.revokeObjectURL` once the row
 * goes away; failing to revoke the URL would leak memory until the
 * tab is reloaded.
 */
export async function resolveIconUrl(
  ref: string | null,
  loader: IconLoader,
  mimeType: string = PNG_MIME,
): Promise<IconResolution> {
  if (!ref) {
    return { ok: false, url: null, blob: null };
  }
  let bytes: number[] | Uint8Array | null;
  try {
    bytes = await loader.loadIconBytes(ref);
  } catch {
    return { ok: false, url: null, blob: null };
  }
  if (bytes == null) {
    return { ok: false, url: null, blob: null };
  }
  const payload = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  // `Blob` requires an `ArrayBuffer`-backed view; `Uint8Array`
  // values returned by the Tauri IPC arrive as plain number arrays
  // (we wrap them above) so the `new Blob` constructor always sees
  // a copy-safe view.
  const blob = new Blob([payload as BlobPart], { type: mimeType });
  const url = URL.createObjectURL(blob);
  return { ok: true, url, blob };
}

export interface IconResolverOptions {
  /**
   * Factory used by the resolver to mint fresh `IconLoader`
   * instances. Defaults to a factory that closes over the supplied
   * `loader` so unit tests can inject a stub loader. Production code
   * passes the real Tauri bridge as `loader` and leaves
   * `loaderFactory` untouched.
   */
  loaderFactory?: () => IconLoader;
  /**
   * MIME type the resolver stamps on the produced `Blob`. Defaults to
   * `image/png` so the existing icon and clipboard-asset consumers
   * stay byte-for-byte identical. The rich-text preview resolver
   * overrides this with `text/html;charset=utf-8` so the sandboxed
   * iframe renders the sanitised fragment as HTML.
   */
  mimeType?: string;
}

export interface IconResolver {
  /**
   * Resolve `ref` to a `blob:` URL, reusing the cached blob when the
   * same `ref` is requested twice in a row.
   */
  resolve(ref: string | null): Promise<IconResolution>;
  /**
   * Release every blob URL the resolver is currently holding. Call
   * this from `onDestroy` so a long-lived panel does not leak memory.
   * The resolver stays usable after a `release` call; the next
   * `resolve` will mint a fresh URL.
   */
  release(): void;
  /**
   * Release only the blob URL associated with `ref`. Use this when a
   * specific row disappears (the user removed the entry from the
   * blacklist).
   */
  releaseFor(ref: string): void;
  /** Map of `ref -> url` currently held by the resolver. Test-only. */
  cacheSize(): number;
}

export function createIconResolver(
  loader: IconLoader,
  options: IconResolverOptions = {},
): IconResolver {
  const factory = options.loaderFactory ?? (() => loader);
  const cache = new Map<string, string>();
  const mimeType = options.mimeType ?? PNG_MIME;
  return {
    async resolve(ref) {
      if (!ref) {
        return { ok: false, url: null, blob: null };
      }
      const cached = cache.get(ref);
      if (cached) {
        return { ok: true, url: cached, blob: null };
      }
      const resolution = await resolveIconUrl(ref, factory(), mimeType);
      if (resolution.ok && resolution.url) {
        cache.set(ref, resolution.url);
      }
      return resolution;
    },
    release() {
      for (const url of cache.values()) {
        URL.revokeObjectURL(url);
      }
      cache.clear();
    },
    releaseFor(ref) {
      const url = cache.get(ref);
      if (url) {
        URL.revokeObjectURL(url);
        cache.delete(ref);
      }
    },
    cacheSize() {
      return cache.size;
    },
  };
}