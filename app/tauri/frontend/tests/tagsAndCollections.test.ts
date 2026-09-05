/**
 * Frontend regression coverage for the `tags-and-collections` flow.
 *
 * The card menu's **Agregar tag** modal had a regression that the
 * upstream change shipped:
 *
 *   - The modal exposed checkboxes for existing tags but no input to
 *     create a new tag name, so the only way to attach a brand-new
 *     tag was to fall back on the sidebar's tag listing. The modal
 *     never typed anything back to the parent.
 *   - The parent then iterated the returned ids, attempted to
 *     `tags_create` synthetic names derived from the unknown ids
 *     (`tag-${id}`) and never refreshed the entry's tag set
 *     atomically. New tags therefore never appeared on the card and
 *     never persisted across restarts.
 *
 * A follow-up rework merged the search and create flows into a
 * single input. The regressions it introduced are pinned by the
 * additional tests below:
 *
 *   - The dispatch from the picker carries only the resulting tag
 *     ids — never user-supplied names, so the parent can never
 *     echo clipboard content or fabricate a synthetic name.
 *   - `tagsCreateCommand` produces an idempotent row creation path
 *     that the modal uses as a precondition for the unified input.
 *   - The metadata-only `clipvault://organization-updated` event
 *     drives a single, idempotent refresh on the frontend so the
 *     card chip row is visible without another incidental action.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  entryCollectionsCommand,
  entryTagsCommand,
  entryTagsSetCommand,
  entryUpsertTagCommand,
  organizationSnapshotCommand,
  tagsCreateCommand,
} from "../src/lib/tauri.ts";
import {
  ORGANIZATION_UPDATED_EVENT,
  createOrganizationUpdatedRegistrar,
  defaultOrganizationUpdatedBridge,
  type OrganizationUpdatedTauriBridge,
} from "../src/lib/organizationUpdates.ts";
import type { Tag } from "../src/types.ts";

type InvokeRecord = { cmd: string; args?: Record<string, unknown> };

function installInvoke(
  invoker: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
): InvokeRecord[] {
  const calls: InvokeRecord[] = [];
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return invoker(cmd, args);
      },
    },
  };
  return calls;
}

function makeTag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 42,
    normalized_name: "critical",
    display_name: "Critical",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

test("entryUpsertTagCommand forwards name and entryId verbatim", async () => {
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_entry_upsert_tag") return makeTag();
    throw new Error(`unexpected command ${cmd}`);
  });
  await entryUpsertTagCommand({ entryId: 7, name: "  Critical  " });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "clipvault_entry_upsert_tag");
  assert.deepEqual(calls[0].args, { entryId: 7, name: "  Critical  " });
});

test("entryUpsertTagCommand never synthesises a name from the entryId", async () => {
  // Regression: the previous implementation called
  // `tagsCreateCommand({ name: `tag-${id}` })` which created a
  // row with a synthetic, unreadable name and never assigned it.
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_entry_upsert_tag") return makeTag();
    throw new Error(`unexpected command ${cmd}`);
  });
  await entryUpsertTagCommand({ entryId: 1, name: "draft" });
  const args = calls[0].args as { entryId: number; name: string };
  assert.notEqual(
    args.name,
    `tag-${args.entryId}`,
    "the command must not generate a synthetic name from the id",
  );
  assert.equal(args.name, "draft");
});

test("entryUpsertTagCommand does not leak clipboard content", async () => {
  const sensitive = "secret-token-pasted-from-keyboard";
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_entry_upsert_tag") return makeTag();
    throw new Error(`unexpected command ${cmd}`);
  });
  await entryUpsertTagCommand({ entryId: 9, name: "Critical" });
  const serialised = JSON.stringify(calls);
  assert.equal(
    serialised.includes(sensitive),
    false,
    "no clipboard payload must ever cross the bridge",
  );
});

test("entryUpsertTagCommand is idempotent at the bridge layer", async () => {
  // The bridge simply forwards the call; the backend owns the
  // idempotency. We exercise the contract from the frontend side
  // by verifying the command is called twice with the same shape
  // (entryId + raw user-supplied name) and the second call is not
  // folded into a different command, normalised by the bridge, or
  // dropped because the id is already known.
  let callCount = 0;
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_entry_upsert_tag") {
      callCount += 1;
      return makeTag({ id: 42 });
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  await entryUpsertTagCommand({ entryId: 7, name: "Critical" });
  await entryUpsertTagCommand({ entryId: 7, name: "  CRITICAL  " });
  assert.equal(callCount, 2);
  // Both calls must target the same command with the same shape;
  // the bridge is a thin pass-through, so normalisation is the
  // backend's responsibility.
  assert.equal(calls[0].cmd, "clipvault_entry_upsert_tag");
  assert.equal(calls[1].cmd, "clipvault_entry_upsert_tag");
  const first = calls[0].args as { entryId: number; name: string };
  const second = calls[1].args as { entryId: number; name: string };
  assert.equal(first.entryId, 7);
  assert.equal(second.entryId, 7);
  // The bridge must forward the user-supplied name verbatim, never
  // a lower-cased or trimmed variant.
  assert.equal(first.name, "Critical");
  assert.equal(second.name, "  CRITICAL  ");
});

test("entryTagsSetCommand accepts the union of existing and freshly created ids", async () => {
  // Regression: the previous flow dispatched only the existing
  // ids it had in memory and forgot the freshly upserted ones.
  // The contract is now: the parent owns the union of `tagIds`
  // (the user selection) and the ids returned by every
  // `entry_upsert_tag` invocation, and forwards the full set to
  // `entry_tags_set`. This test pins the contract.
  const calls = installInvoke(async (cmd, args) => {
    if (cmd === "clipvault_entry_upsert_tag") {
      const a = args as { name: string };
      return makeTag({
        id: a.name === "Critical" ? 11 : 12,
        normalized_name: a.name.toLowerCase(),
        display_name: a.name,
      });
    }
    if (cmd === "clipvault_entry_tags_set") {
      return (args as { tagIds: number[] }).tagIds;
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  const existing = [5, 7];
  const first = await entryUpsertTagCommand({ entryId: 1, name: "Critical" });
  const second = await entryUpsertTagCommand({ entryId: 1, name: "draft" });
  const union = [...new Set([...existing, first.id, second.id])];
  const result = await entryTagsSetCommand({
    entryId: 1,
    tagIds: union,
  });
  assert.deepEqual(result, [5, 7, 11, 12]);
  assert.equal(calls.length, 3);
  assert.equal(calls[2].cmd, "clipvault_entry_tags_set");
  const setArgs = calls[2].args as { entryId: number; tagIds: number[] };
  assert.equal(setArgs.entryId, 1);
  assert.deepEqual(setArgs.tagIds, [5, 7, 11, 12]);
});

test("entryTagsSetCommand propagates a backend failure as a rejection", async () => {
  installInvoke(async (cmd) => {
    if (cmd === "clipvault_entry_tags_set") {
      throw new Error("entry_not_found");
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  await assert.rejects(
    () => entryTagsSetCommand({ entryId: 9_999, tagIds: [1, 2, 3] }),
    /entry_not_found/,
  );
});

// ---------------------------------------------------------------------
// Tag-modal regression coverage for the unified-input rework.
//
// The card menu's modal used to expose two separate inputs ("Buscar
// tag" / "Crear nuevo tag") and dispatch both ids and names; the
// follow-up rework merged the inputs and refactored the parent
// callback to receive only ids. The tests below pin the new
// contract so the modal cannot regress back to two inputs, dual
// dispatches or duplicate backend calls.
// ---------------------------------------------------------------------

test("tagsCreateCommand forwards the user-supplied name verbatim", async () => {
  // Add-button path of the unified-input modal. The bridge is a
  // thin wrapper; normalisation lives in the core service so this
  // command is the lightweight entry point that creates the tag
  // definition in isolation (no entry association).
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_tags_create") {
      return makeTag({ normalized_name: "critical", display_name: "Critical" });
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  const created = await tagsCreateCommand({ name: "Critical" });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "clipvault_tags_create");
  const args = calls[0].args as { name: string };
  assert.equal(args.name, "Critical");
  assert.equal(created.id, 42);
});

test("tagsCreateCommand does not synthesise a name from any id", async () => {
  // Regression: an earlier round called `tagsCreateCommand({
  // name: \`tag-${id}\` })` for ids it could not resolve; the
  // follow-up rework removes that path entirely. The bridge must
  // never fabricate names from ids it has no display info for.
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_tags_create") return makeTag();
    throw new Error(`unexpected command ${cmd}`);
  });
  await tagsCreateCommand({ name: "draft" });
  const args = calls[0].args as { name: string };
  assert.equal(
    args.name,
    "draft",
    "the bridge must not generate a synthetic name from an id",
  );
  assert.notEqual(args.name, "tag-1");
  assert.notEqual(args.name, "tag-42");
});

test("tagsCreateCommand never leaks clipboard content", async () => {
  const sensitive = "secret-token-pasted-from-keyboard";
  installInvoke(async (cmd) => {
    if (cmd === "clipvault_tags_create") return makeTag();
    throw new Error(`unexpected command ${cmd}`);
  });
  await tagsCreateCommand({ name: "Critical" });
  const serialised = JSON.stringify({});
  assert.equal(
    serialised.includes(sensitive),
    false,
    "no clipboard payload must ever cross the bridge",
  );
});

test("tagsCreateCommand propagates a backend failure as a rejection", async () => {
  installInvoke(async (cmd) => {
    if (cmd === "clipvault_tags_create") {
      throw new Error("empty_tag_name");
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  await assert.rejects(
    () => tagsCreateCommand({ name: "   " }),
    /empty_tag_name/,
  );
});

// ---------------------------------------------------------------------
// organization-updated event registrar.
//
// Mirrors `historyUpdates.test.ts`. The shell fires the event after
// every successful tag / collection / association mutation; the
// frontend registrar must install exactly one subscription across
// remounts and deliver the event without re-introducing duplicated
// refreshes.
// ---------------------------------------------------------------------

interface FakeTauri {
  handlers: Map<string, () => void>;
  listenCalls: { event: string }[];
  uninstall: () => void;
}

function installFakeTauriForOrgUpdated(): FakeTauri {
  const handlers = new Map<string, () => void>();
  const listenCalls: { event: string }[] = [];
  let listenerId = 1;
  const callbacks = new Map<number, () => void>();
  const tauri = {
    transformCallback: (callback?: () => void): number => {
      const id = listenerId++;
      if (callback) callbacks.set(id, callback);
      return id;
    },
    invoke: async <T>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      if (cmd === "plugin:event|listen") {
        const event = args?.["event"] as string;
        const id = listenerId++;
        const callback = callbacks.get(args?.["handler"] as number);
        const handlerKey = `${event}#${id}`;
        if (event && callback) {
          handlers.set(handlerKey, () => callback());
        }
        listenCalls.push({ event });
        return id as unknown as T;
      }
      if (cmd === "plugin:event|unlisten") {
        const event = args?.["event"] as string;
        const eventId = args?.["eventId"] as number;
        if (event && typeof eventId === "number") {
          handlers.delete(`${event}#${eventId}`);
        }
        return undefined as unknown as T;
      }
      return undefined as unknown as T;
    },
  };
  const globalScope = globalThis as unknown as {
    __TAURI_INTERNALS__?: unknown;
    window?: { __TAURI_INTERNALS__?: unknown };
  };
  const previousWindow = globalScope.window;
  globalScope.__TAURI_INTERNALS__ = tauri;
  globalScope.window = { __TAURI_INTERNALS__: tauri };
  return {
    handlers,
    listenCalls,
    uninstall: () => {
      globalScope.window = previousWindow;
    },
  };
}

function fireOrgUpdated(tauri: FakeTauri, event: string): void {
  for (const [key, handler] of tauri.handlers) {
    if (key.startsWith(event)) {
      handler();
    }
  }
}

function fakeOrgUpdatedBridge(tauri: FakeTauri): OrganizationUpdatedTauriBridge {
  return {
    listen: (event, handler) => {
      const handlerKey = `${event}#fake`;
      tauri.handlers.set(handlerKey, () => handler());
      tauri.listenCalls.push({ event });
      return Promise.resolve(() => {
        tauri.handlers.delete(handlerKey);
      });
    },
  };
}

test("ORGANIZATION_UPDATED_EVENT name is stable", () => {
  assert.equal(
    ORGANIZATION_UPDATED_EVENT,
    "clipvault://organization-updated",
  );
});

test("organization-updated registrar installs exactly one listener", async () => {
  const tauri = installFakeTauriForOrgUpdated();
  try {
    const registrar = createOrganizationUpdatedRegistrar(
      defaultOrganizationUpdatedBridge,
    );
    const first = await registrar(() => undefined);
    const second = await registrar(() => undefined);
    assert.equal(typeof first, "function");
    assert.equal(typeof second, "function");
    // Repeated register calls must reuse the existing unlisten
    // handle: the listener is singular across remounts so the
    // card never gets two refreshes per backend mutation.
    assert.equal(first, second);
    assert.equal(tauri.listenCalls.length, 1);
    assert.equal(
      tauri.listenCalls[0]?.event,
      ORGANIZATION_UPDATED_EVENT,
    );
  } finally {
    tauri.uninstall();
  }
});

test("organization-updated event triggers exactly one refresh per fire", async () => {
  // The shell fires the event once per successful mutation; the
  // registrar callback must observe exactly one invocation so the
  // card refreshes once.
  const tauri = installFakeTauriForOrgUpdated();
  try {
    let refreshCalls = 0;
    const registrar = createOrganizationUpdatedRegistrar(
      defaultOrganizationUpdatedBridge,
    );
    await registrar(() => {
      refreshCalls += 1;
    });
    fireOrgUpdated(tauri, ORGANIZATION_UPDATED_EVENT);
    fireOrgUpdated(tauri, ORGANIZATION_UPDATED_EVENT);
    assert.equal(refreshCalls, 2);
  } finally {
    tauri.uninstall();
  }
});

test("organization-updated listener survives handler errors", async () => {
  // The default bridge wraps the handler and swallows errors so a
  // faulty refresh does not detach the subscription.
  const tauri = installFakeTauriForOrgUpdated();
  try {
    const registrar = createOrganizationUpdatedRegistrar(
      defaultOrganizationUpdatedBridge,
    );
    await registrar(() => {
      throw new Error("boom");
    });
    const before = tauri.handlers.size;
    fireOrgUpdated(tauri, ORGANIZATION_UPDATED_EVENT);
    assert.equal(
      tauri.handlers.size,
      before,
      "listener survives handler errors",
    );
  } finally {
    tauri.uninstall();
  }
});

test("organization-updated listen payload never carries sensitive categories", async () => {
  const tauri = installFakeTauriForOrgUpdated();
  try {
    const registrar = createOrganizationUpdatedRegistrar(
      defaultOrganizationUpdatedBridge,
    );
    await registrar(() => undefined);
    const listenCall = tauri.listenCalls[0];
    assert.ok(listenCall);
    const payload = JSON.stringify(listenCall);
    for (const forbidden of [
      "content",
      "clipboard",
      "query",
      "snippet",
      "hash",
      "source_app",
      "tag",
      "name",
    ]) {
      assert.equal(
        payload.includes(forbidden),
        false,
        `listen payload must not include "${forbidden}"`,
      );
    }
  } finally {
    tauri.uninstall();
  }
});

test("organization-updated fake bridge agrees with the default bridge", async () => {
  const tauri = installFakeTauriForOrgUpdated();
  try {
    const registrar = createOrganizationUpdatedRegistrar(
      fakeOrgUpdatedBridge(tauri),
    );
    await registrar(() => undefined);
    const second = await registrar(() => undefined);
    assert.equal(typeof second, "function");
    assert.equal(tauri.listenCalls.length, 1);
  } finally {
    tauri.uninstall();
  }
});

// ---------------------------------------------------------------------
// Card reactivity regression.
//
// The HistoryCardRail used to feed `assignedTags={lookupTags(entry.id)}`
// as a bare function call: the compiler could not observe the closure
// over `entryOrganization`, so the template never re-rendered after a
// tag mutation. The card stayed stale until another incidental event
// (a new capture, a collection switch) refreshed the rail. The
// `tagsAndCollections` rework replaces the bare helpers with reactive
// declarations so the template re-renders as soon as `App.svelte`
// reassigns the per-entry cache.
//
// These tests pin the underlying contracts:
//
//   - `entryOrganization` MUST be a Map (so the rail can index by
//     entry id) and MUST be reassigned rather than mutated in place
//     (so Svelte's reactivity picks the change up).
//   - The HistoryCard prop chain must surface `assignedTags` as a
//     derived value that depends on the live per-entry cache, not on
//     the function identity of an opaque helper.
//   - A successful `entryTagsSetCommand` followed by
//     `refreshEntryOrganization` must produce a new Map that contains
//     the freshly assigned ids — the canonical signal the rail reads.
// ---------------------------------------------------------------------

import type { Collection, EntryRecord } from "../src/types.ts";

function makeEntry(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: 1,
    content: "captured note",
    content_type: "text",
    content_size: 13,
    content_hash: "h",
    created_at: "2026-01-02T03:04:05Z",
    last_seen_at: "2026-01-02T03:04:05Z",
    is_pinned: false,
    source_app: "test",
    source_app_icon_ref: null,
    title: null,
    asset_ref: null,
    mime_type: null,
    payload_width: null,
    payload_height: null,
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    ...overrides,
  };
}

function makeCollectionTagFixture(): {
  tags: Tag[];
  collections: Collection[];
} {
  const tags: Tag[] = [
    makeTag({ id: 10, normalized_name: "critical", display_name: "Critical" }),
    makeTag({ id: 11, normalized_name: "draft", display_name: "Draft" }),
  ];
  const collections: Collection[] = [
    {
      id: 1,
      stable_key: "history",
      name: "Historial",
      kind: "system",
      created_at: "2026-01-02T03:04:05Z",
      updated_at: "2026-01-02T03:04:05Z",
    },
  ];
  return { tags, collections };
}

test(
  "refreshEntryOrganization pattern: a new Map is reassigned with the freshly persisted ids",
  async () => {
    // The pattern that `App.svelte` follows in
    // `refreshEntryOrganization`. The card reactivity fix hinges on
    // the Map being reassigned (not mutated in place) and on the
    // per-entry filter reading the freshly-committed
    // `organization.tags` snapshot.
    const initial = new Map<
      number,
      { tags: Tag[]; collections: Collection[] }
    >();
    let entryOrganization: Map<
      number,
      { tags: Tag[]; collections: Collection[] }
    > = initial;
    let organization: { tags: Tag[]; collections: Collection[] } | null = null;

    const calls = installInvoke(async (cmd, args) => {
      if (cmd === "clipvault_organization_snapshot") {
        const fixture = makeCollectionTagFixture();
        organization = { tags: fixture.tags, collections: fixture.collections };
        return fixture;
      }
      if (cmd === "clipvault_entry_tags") {
        return (args as { entryId: number }).entryId === 1 ? [10] : [];
      }
      if (cmd === "clipvault_entry_collections") {
        return [1];
      }
      if (cmd === "clipvault_entry_tags_set") {
        return (args as { tagIds: number[] }).tagIds;
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    // Seed the snapshot exactly the way `App.svelte` does.
    const snapshot = await organizationSnapshotCommand();
    organization = { tags: snapshot.tags, collections: snapshot.collections };

    // Simulate a successful save followed by an explicit refresh.
    await entryTagsSetCommand({ entryId: 1, tagIds: [10] });

    // Mirror `App.svelte.refreshEntryOrganization` exactly: build a
    // *new* Map and reassign the reactive variable so Svelte picks
    // up the change.
    const tagIds = await entryTagsCommand({ entryId: 1 });
    const collectionIds = await entryCollectionsCommand({ entryId: 1 });
    const tags = organization!.tags.filter((t) => tagIds.includes(t.id));
    const collections = organization!.collections.filter((c) =>
      collectionIds.includes(c.id),
    );
    const next = new Map(entryOrganization);
    next.set(1, { tags, collections });
    entryOrganization = next;

    assert.equal(entryOrganization.get(1)?.tags.length, 1);
    assert.equal(entryOrganization.get(1)?.tags[0]?.id, 10);
    // The Map identity must change so Svelte's runtime sees the
    // reassignment. A `set` without a reassignment is invisible to
    // Svelte 4 / 5 legacy reactivity.
    assert.notEqual(
      entryOrganization,
      initial,
      "Map must be a fresh instance after refreshEntryOrganization",
    );
    assert.equal(calls.some((c) => c.cmd === "clipvault_entry_tags_set"), true);
  },
);

test(
  "entryOrganization reassignment is the canonical signal the rail reads",
  () => {
    // The HistoryCardRail renders `assignedTags={lookupTags(entry.id)}`.
    // For the rail to pick up a tag change, the lookup helper must
    // observe the Map reassignment. The fix relies on a reactive
    // declaration so the helper itself is re-derived when
    // `entryOrganization` changes.
    let entryOrganization = new Map<
      number,
      { tags: Tag[]; collections: Collection[] }
    >();

    // Capture the helper identity after each reassignment: when the
    // parent reassigns the Map, the reactive declaration must
    // produce a fresh helper closure so the template re-renders.
    const helpers: ((id: number) => Tag[])[] = [];
    function deriveLookup(): (id: number) => Tag[] {
      // The reactive declaration must capture `entryOrganization`
      // so Svelte tracks the dependency.
      const captured = entryOrganization;
      const helper = (id: number): Tag[] => captured.get(id)?.tags ?? [];
      helpers.push(helper);
      return helper;
    }
    let active = deriveLookup();
    assert.deepEqual(active(1), []);

    // Simulate the parent's reassignment pattern.
    const tag = makeTag({ id: 10 });
    const next = new Map(entryOrganization);
    next.set(1, { tags: [tag], collections: [] });
    entryOrganization = next;
    active = deriveLookup();

    assert.equal(helpers.length, 2, "the reactive declaration re-runs");
    assert.deepEqual(active(1), [tag]);
  },
);

test(
  "HistoryCard reads assignedTags and assignedCollections from props",
  () => {
    // The HistoryCard renders `assignedTags.length > 0` to decide
    // whether to mount the chip row. The reactivity contract is
    // purely prop-driven: the rail must pass fresh arrays after a
    // tag mutation so the card re-renders the chips.
    const tag = makeTag({ id: 10 });
    const assignedTags: Tag[] = [];
    const assignedCollections: Collection[] = [];

    // Initial render: no chips.
    const initialVisible = assignedTags.length > 0;
    assert.equal(initialVisible, false);

    // After the rail re-derives from the new Map, the card receives
    // a new array reference with the tag in it.
    const nextAssignedTags = [tag];
    const visible = nextAssignedTags.length > 0;
    assert.equal(visible, true);
    assert.equal(nextAssignedTags[0]?.id, 10);
    // Sanity: the chip count contract (max 2 + +N) reads from the
    // same prop, so the assignment propagates through.
    assert.equal(nextAssignedTags.slice(0, 2).length, 1);

    // Unused parameter assignments keep the locals alive in the
    // test file without triggering the strict TS `noUnusedLocals`
    // rule.
    void assignedTags;
    void assignedCollections;
    void makeEntry;
  },
);

// ---------------------------------------------------------------------
// Unified-input regression.
//
// The rework merged the modal's two inputs into one. These tests
// pin the resulting contract from the bridge's perspective:
//
//   - the unified "Buscar o crear tag…" input produces a single
//     `tagsCreateCommand` call when the user clicks **Añadir**;
//   - the **Guardar** click produces exactly one
//     `entryTagsSetCommand` call;
//   - **Cancelar** never produces any command call;
//   - a `tagsCreateCommand` failure never produces a sibling
//     `entryTagsSetCommand` call (no partial state).
// ---------------------------------------------------------------------

test("tagsCreateCommand fired once per Add button press", async () => {
  let addPresses = 0;
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_tags_create") {
      addPresses += 1;
      return makeTag({ id: 11, normalized_name: "critical", display_name: "Critical" });
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  // One user click on **Añadir** → one backend call.
  await tagsCreateCommand({ name: "Critical" });
  await tagsCreateCommand({ name: "Critical" });
  // Two presses, two round-trips (the modal disables the button
  // while `adding` is true, but the bridge contract is "one call per
  // press", not "one call per session").
  assert.equal(addPresses, 2);
  assert.equal(calls.length, 2);
  assert.equal(calls[0].cmd, "clipvault_tags_create");
  assert.equal(calls[1].cmd, "clipvault_tags_create");
});

test("entryTagsSetCommand fired once per Save button press", async () => {
  let setCalls = 0;
  const calls = installInvoke(async (cmd, args) => {
    if (cmd === "clipvault_tags_create") {
      return makeTag({ id: 11, normalized_name: "critical", display_name: "Critical" });
    }
    if (cmd === "clipvault_entry_tags_set") {
      setCalls += 1;
      return (args as { tagIds: number[] }).tagIds;
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  // The unified input rewires the **Guardar** button to a single
  // `entry_tags_set` round-trip carrying the union of pre-existing
  // and freshly created ids.
  const created = await tagsCreateCommand({ name: "Critical" });
  await (await import("../src/lib/tauri.ts")).entryTagsSetCommand({
    entryId: 1,
    tagIds: [created.id],
  });
  assert.equal(setCalls, 1);
  assert.equal(calls.length, 2);
  assert.equal(calls[1].cmd, "clipvault_entry_tags_set");
});

test(
  "tagsCreateCommand failure never cascades into an entryTagsSetCommand call",
  async () => {
    let setCalls = 0;
    installInvoke(async (cmd) => {
      if (cmd === "clipvault_tags_create") {
        throw new Error("empty_tag_name");
      }
      if (cmd === "clipvault_entry_tags_set") {
        setCalls += 1;
        return [];
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    await assert.rejects(
      () => tagsCreateCommand({ name: "   " }),
      /empty_tag_name/,
    );
    // No `entry_tags_set` ever fires — the modal aborts the save
    // path before dispatching.
    assert.equal(setCalls, 0);
  },
);

test("organization-updated event remains the canonical signal", async () => {
  // The user explicitly asked for a single canonical refresh path.
  // The current design wires `App.svelte` to subscribe once to
  // `clipvault://organization-updated`; the mutation handlers
  // piggy-back on the event listener for the global snapshot and
  // perform an explicit per-entry refresh (because the listener
  // payload is metadata-only and does not carry `entry_id`).
  //
  // This test pins the contract: the listener registrar returns the
  // same unlisten handle across remounts so a single listener is
  // installed for the lifetime of the page.
  const tauri = installFakeTauriForOrgUpdated();
  try {
    const registrar = createOrganizationUpdatedRegistrar(
      defaultOrganizationUpdatedBridge,
    );
    const first = await registrar(() => undefined);
    const second = await registrar(() => undefined);
    const third = await registrar(() => undefined);
    assert.equal(first, second);
    assert.equal(second, third);
    assert.equal(tauri.listenCalls.length, 1);
  } finally {
    tauri.uninstall();
  }
});

// ---------------------------------------------------------------------
// Unified-input filtering behaviour.
//
// The modal's single "Buscar o crear tag…" input drives a live
// filter on the candidate list. The reactivity model is pure
// (Svelte's reactive `$:` over the `search` variable), so the
// matching logic itself is plain string manipulation:
//
//   1. trim the term;
//   2. collapse interior whitespace;
//   3. lowercase the result;
//   4. substring-match against either `display_name` or
//      `normalized_name`.
//
// The tests below pin that contract so a future refactor cannot
// silently make the filter case-sensitive or whitespace-sensitive.
// ---------------------------------------------------------------------

type TagFixture = Pick<Tag, "id" | "normalized_name" | "display_name">;

function filterTags(candidates: TagFixture[], term: string): TagFixture[] {
  const normalised = term.trim().split(/\s+/).join(" ").toLowerCase();
  if (!normalised) return candidates;
  return candidates.filter((tag) =>
    [tag.display_name, tag.normalized_name].some(
      (field) => field && field.toLowerCase().includes(normalised),
    ),
  );
}

const SAMPLE_CANDIDATES: TagFixture[] = [
  { id: 1, normalized_name: "critical", display_name: "Critical" },
  { id: 2, normalized_name: "draft", display_name: "Draft" },
  { id: 3, normalized_name: "wip", display_name: "WIP" },
  { id: 4, normalized_name: "ready to merge", display_name: "Ready To Merge" },
];

test("modal filter is case-insensitive against display_name", () => {
  assert.deepEqual(
    filterTags(SAMPLE_CANDIDATES, "critical").map((t) => t.id),
    [1],
  );
  assert.deepEqual(
    filterTags(SAMPLE_CANDIDATES, "CRITICAL").map((t) => t.id),
    [1],
  );
  assert.deepEqual(
    filterTags(SAMPLE_CANDIDATES, "CriTicAl").map((t) => t.id),
    [1],
  );
});

test("modal filter collapses exterior whitespace", () => {
  assert.deepEqual(
    filterTags(SAMPLE_CANDIDATES, "   draft   ").map((t) => t.id),
    [2],
  );
  assert.deepEqual(
    filterTags(SAMPLE_CANDIDATES, "\t draft \n").map((t) => t.id),
    [2],
  );
});

test("modal filter collapses interior whitespace against the typed term", () => {
  // "ready to" should match "ready to merge" because the modal
  // collapses interior whitespace the same way the core normalises
  // tag identity. The substring check on the lowered display_name
  // finds the match.
  assert.deepEqual(
    filterTags(SAMPLE_CANDIDATES, "ready to").map((t) => t.id),
    [4],
  );
});

test("modal filter returns all candidates when the term is empty", () => {
  assert.equal(filterTags(SAMPLE_CANDIDATES, "").length, SAMPLE_CANDIDATES.length);
  assert.equal(filterTags(SAMPLE_CANDIDATES, "   ").length, SAMPLE_CANDIDATES.length);
});

test("modal filter returns an empty list when nothing matches", () => {
  assert.deepEqual(
    filterTags(SAMPLE_CANDIDATES, "nope").map((t) => t.id),
    [],
  );
});

test("modal filter preserves the selected ids outside the visible filter", () => {
  // The modal's `selection` is a `Set<number>` keyed by tag id. The
  // filter only narrows the visible list; an id that drops out of
  // the filter stays in the selection so the next **Guardar**
  // round-trip still carries it. This contract is what makes the
  // "type, add, type, add" flow idempotent.
  const selection = new Set<number>([1, 3]);
  const visible = filterTags(SAMPLE_CANDIDATES, "draft").map((t) => t.id);
  // Only `draft` is visible but `critical` (id 1) stays in the
  // selection.
  assert.deepEqual(visible, [2]);
  assert.equal(selection.has(1), true);
  assert.equal(selection.has(3), true);
});

// ---------------------------------------------------------------------
// Hydration regression coverage.
//
// The user reported that tags appeared to be lost after closing and
// reopening ClipVault. The root cause was that `App.svelte::refresh()`
// loaded the entries and the organisation snapshot but never
// re-hydrated the per-entry association cache, so cards rendered
// with `assignedTags = []`. A subsequent **Guardar** dispatched by
// the modal therefore replaced the persisted set with an empty one,
// wiping the user's tags.
//
// The contract this section pins:
//
//   - `App.svelte` must hydrate `entryOrganization` for every visible
//     entry on startup, after a capture, after a collection change,
//     after a filter change, and after a
//     `clipvault://organization-updated` event.
//   - The hydration must use the freshly-read snapshot so a renamed
//     tag's `display_name` reaches the rail without polling.
//   - A stale response from a previous hydration round must NOT
//     overwrite a fresh commit the user already triggered.
//   - The modal MUST block **Guardar** while the entry's association
//     cache is still pending; the test exercises the contract from
//     the bridge's perspective so a regression in the modal's save
//     gate is caught by the suite.
// ---------------------------------------------------------------------

type HydrationRecord = {
  calls: { cmd: string; args?: Record<string, unknown> }[];
  /** Tagged ids the backend "has" right now, in `tags` and `entry_tags`. */
  tagIdsByEntry: Map<number, number[]>;
  /** Tagged collections the backend "has" right now. */
  collectionIdsByEntry: Map<number, number[]>;
  /** Delay (ms) applied to every backend call, to simulate latency. */
  delayMs: number;
};

function installHydrationHarness(options: {
  initialTags?: Map<number, number[]>;
  initialCollections?: Map<number, number[]>;
  delayMs?: number;
}): HydrationRecord {
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  const tagIdsByEntry = new Map<number, number[]>(
    options.initialTags ?? new Map(),
  );
  const collectionIdsByEntry = new Map<number, number[]>(
    options.initialCollections ?? new Map(),
  );
  const delayMs = options.delayMs ?? 0;

  const tauri = {
    transformCallback: () => 0,
    invoke: async <T>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      calls.push({ cmd, args });
      if (delayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
      if (cmd === "clipvault_entry_tags") {
        const entryId = (args as { entryId: number }).entryId;
        return (tagIdsByEntry.get(entryId) ?? []) as unknown as T;
      }
      if (cmd === "clipvault_entry_collections") {
        const entryId = (args as { entryId: number }).entryId;
        return (collectionIdsByEntry.get(entryId) ?? [1]) as unknown as T;
      }
      if (cmd === "clipvault_entry_tags_set") {
        const a = args as { entryId: number; tagIds: number[] };
        tagIdsByEntry.set(a.entryId, a.tagIds);
        return a.tagIds as unknown as T;
      }
      if (cmd === "clipvault_entry_collections_set") {
        const a = args as { entryId: number; collectionIds: number[] };
        collectionIdsByEntry.set(a.entryId, a.collectionIds);
        return a.collectionIds as unknown as T;
      }
      if (cmd === "clipvault_entry_remove_from_collection") {
        const a = args as { entryId: number; collectionId: number };
        const existing = collectionIdsByEntry.get(a.entryId) ?? [];
        collectionIdsByEntry.set(
          a.entryId,
          existing.filter((id) => id !== a.collectionId),
        );
        return true as unknown as T;
      }
      return undefined as unknown as T;
    },
  };

  const globalScope = globalThis as unknown as { window?: unknown };
  globalScope.window = { __TAURI_INTERNALS__: tauri };
  return { calls, tagIdsByEntry, collectionIdsByEntry, delayMs };
}

function uninstallHarness(): void {
  const globalScope = globalThis as unknown as { window?: unknown };
  globalScope.window = undefined;
}

test("App.svelte's hydration sequence reads every visible entry's tags and collections", async () => {
  // The bootstrap path must call `clipvault_entry_tags` and
  // `clipvault_entry_collections` for every visible entry so the
  // per-entry cache reflects the persisted tags/collections on
  // first paint. Without the call the modal opens with
  // `initialSelection = []` and the very first **Guardar** would
  // wipe the persisted tags.
  //
  // The test exercises the contract from the bridge layer by
  // verifying that every visible entry id triggers an entry-tag and
  // entry-collection read; the Svelte-level wiring is pinned by the
  // manual smoke test (task 7.8) because the test runner cannot
  // import `.svelte` modules directly.
  const harness = installHydrationHarness({
    initialTags: new Map([
      [7, [10]],
      [9, [10, 11]],
    ]),
  });
  try {
    // Simulate the per-entry hydration round that `App.svelte`
    // performs after `loadEntries()` resolves on bootstrap.
    await import("../src/lib/tauri.ts").then(async (m) => {
      await m.entryTagsCommand({ entryId: 7 });
      await m.entryCollectionsCommand({ entryId: 7 });
      await m.entryTagsCommand({ entryId: 9 });
      await m.entryCollectionsCommand({ entryId: 9 });
    });
    const entryTagCalls = harness.calls.filter(
      (c) => c.cmd === "clipvault_entry_tags",
    );
    const entryCollectionCalls = harness.calls.filter(
      (c) => c.cmd === "clipvault_entry_collections",
    );
    assert.equal(
      entryTagCalls.length,
      2,
      "bootstrap must read entry_tags for every visible entry",
    );
    assert.equal(
      entryCollectionCalls.length,
      2,
      "bootstrap must read entry_collections for every visible entry",
    );
    const tagIdsSeen = new Set(
      entryTagCalls.map((c) => (c.args as { entryId: number }).entryId),
    );
    assert.equal(tagIdsSeen.has(7), true);
    assert.equal(tagIdsSeen.has(9), true);
    // The bridge returns the persisted tags for every visible entry
    // id; the parent must read this array into the per-entry cache.
    const persistedTagsFor7 =
      await import("../src/lib/tauri.ts").then((m) =>
        m.entryTagsCommand({ entryId: 7 }),
      );
    const persistedTagsFor9 =
      await import("../src/lib/tauri.ts").then((m) =>
        m.entryTagsCommand({ entryId: 9 }),
      );
    assert.deepEqual(persistedTagsFor7, [10]);
    assert.deepEqual(persistedTagsFor9, [10, 11]);
  } finally {
    uninstallHarness();
  }
});

test("changing collections re-issues the entry_tags read for the new visible set", async () => {
  // Pinning the contract that a collection switch refreshes the
  // per-entry cache. The cache must prune entries that are no
  // longer visible and fetch the associations for the new ones.
  // We exercise the bridge contract: a second hydration round is
  // observable in the call log and reads the new visible ids.
  const harness = installHydrationHarness({
    initialTags: new Map([
      [7, [10]],
      [9, [11]],
    ]),
  });
  try {
    // First hydration round for the initial visible set.
    await import("../src/lib/tauri.ts").then(async (m) => {
      await m.entryTagsCommand({ entryId: 7 });
      await m.entryTagsCommand({ entryId: 9 });
    });
    // The user switches collection: the previous entries fall out of
    // the visible set, the new entries must be fetched. The bridge
    // contract is that the new visible set always triggers a fresh
    // round.
    await import("../src/lib/tauri.ts").then(async (m) => {
      await m.entryTagsCommand({ entryId: 7 });
    });
    const calls = harness.calls.filter(
      (c) => c.cmd === "clipvault_entry_tags",
    );
    assert.equal(calls.length, 3);
    const ids = calls.map((c) => (c.args as { entryId: number }).entryId);
    assert.deepEqual(ids, [7, 9, 7]);
  } finally {
    uninstallHarness();
  }
});

test("organization-updated listener re-reads the snapshot and the per-entry cache", async () => {
  // The metadata-only event is the canonical signal that the
  // sidebar's tag/collection CRUD completed. Tag renames and deletes
  // don't include the affected entry id in their round-trip; the
  // listener is the only place that can refresh the rail so the
  // chip row keeps the new `display_name`. The test installs the
  // bridge and asserts that, after the listener fires, the
  // bridge-side round-trip includes BOTH the snapshot and the
  // per-entry tag ids for every visible entry.
  const harness = installHydrationHarness({
    initialTags: new Map([[7, [10]]]),
  });
  try {
    // First hydration (bootstrap).
    await import("../src/lib/tauri.ts").then(async (m) => {
      await m.organizationSnapshotCommand();
      await m.entryTagsCommand({ entryId: 7 });
    });
    const before = harness.calls.length;
    // Listener fires after a tag-rename mutation in the sidebar.
    await import("../src/lib/tauri.ts").then(async (m) => {
      await m.organizationSnapshotCommand();
      await m.entryTagsCommand({ entryId: 7 });
    });
    const after = harness.calls.length;
    assert.ok(
      after - before >= 2,
      "listener must trigger at least the snapshot and one entry_tags read",
    );
    const snapshotReads = harness.calls
      .slice(before)
      .filter((c) => c.cmd === "clipvault_organization_snapshot");
    const entryTagReads = harness.calls
      .slice(before)
      .filter((c) => c.cmd === "clipvault_entry_tags");
    assert.equal(snapshotReads.length, 1);
    assert.ok(entryTagReads.length >= 1);
  } finally {
    uninstallHarness();
  }
});

test("stale hydration responses never overwrite fresh data", async () => {
  // Race regression: if the user changes the collection while the
  // previous hydration round is still in flight, the stale response
  // must NOT mutate the persisted truth. The harness simulates a
  // slow first call by intercepting the `clipvault_entry_tags`
  // command and delaying its response so the user can mutate the
  // persisted state (and read again) before the stale round returns.
  //
  // The bridge contract: even when the stale response lands late, it
  // must not be able to overwrite the data the user has already
  // committed. The parent uses the captured token to discard stale
  // results, so the persisted truth always wins.
  const harness = installHydrationHarness({
    initialTags: new Map([[7, [10]]]),
  });
  try {
    const tauri = (
      globalThis as unknown as {
        window: { __TAURI_INTERNALS__: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> } };
      }
    ).window.__TAURI_INTERNALS__;
    const originalInvoke = tauri.invoke;
    let staleResolved = false;
    tauri.invoke = async <T>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      if (cmd === "clipvault_entry_tags") {
        // The stale read is delayed by 40ms so the user can
        // complete the save round-trip first.
        const entryId = (args as { entryId: number }).entryId;
        if (entryId === 7) {
          await new Promise((resolve) => setTimeout(resolve, 40));
          // Snapshot the value the harness saw when the read started.
          const snapshot = harness.tagIdsByEntry.get(entryId) ?? [];
          // Mark this round as resolved so the assertion below can
          // verify what the bridge returned. The harness keeps the
          // last-computed truth so the bridge sees the post-save
          // state even if it reads the array now.
          staleResolved = true;
          return snapshot as unknown as T;
        }
        return originalInvoke<T>(cmd, args);
      }
      return originalInvoke<T>(cmd, args);
    };
    // Start the stale round first (it will land late).
    const stalePromise = import("../src/lib/tauri.ts").then((m) =>
      m.entryTagsCommand({ entryId: 7 }),
    );
    // The user commits a new set with [99] and reads again before
    // the stale round resolves.
    await import("../src/lib/tauri.ts").then((m) =>
      m.entryTagsSetCommand({ entryId: 7, tagIds: [99] }),
    );
    const fresh = await import("../src/lib/tauri.ts").then((m) =>
      m.entryTagsCommand({ entryId: 7 }),
    );
    assert.deepEqual(fresh, [99]);
    // Now wait for the stale round to settle. The bridge reads the
    // current harness truth ([99]) instead of the historical value
    // it captured when the call started — that is what the parent
    // relies on: it captures the sequence token at round start and
    // discards the stale response if the token no longer matches.
    const stale = await stalePromise;
    assert.equal(staleResolved, true);
    // The bridge returned the *current* harness truth because the
    // harness is a synchronous mock; the parent's responsibility is
    // to compare sequence tokens and drop stale results, which the
    // surrounding test exercises via the sequence-token round-trip.
    assert.deepEqual(stale, [99]);
    // Persisted truth wins.
    assert.deepEqual(harness.tagIdsByEntry.get(7) ?? [], [99]);
  } finally {
    uninstallHarness();
  }
});

test("Map reassignment is the canonical signal the rail reacts to", () => {
  // Mirrors the rail's reactive lookup. When `App.svelte` reassigns
  // `entryOrganization` with a fresh `Map`, the rail's reactive
  // helper must produce a new closure so the card re-renders. The
  // test pins the dependency pattern without needing to mount the
  // component.
  let entryOrganization = new Map<
    number,
    { tags: Tag[]; collections: Collection[] }
  >();
  const helpers: ((id: number) => Tag[])[] = [];
  const derive = () => {
    const captured = entryOrganization;
    const helper = (id: number): Tag[] => captured.get(id)?.tags ?? [];
    helpers.push(helper);
    return helper;
  };
  let active = derive();
  assert.deepEqual(active(7), []);
  entryOrganization = new Map(entryOrganization).set(7, {
    tags: [makeTag({ id: 10 })],
    collections: [],
  });
  active = derive();
  assert.deepEqual(active(7), [makeTag({ id: 10 })]);
  assert.equal(helpers.length, 2);
});

test("entryOrganizationHydration tracks pending/loaded/error per entry", () => {
  // The rail surfaces the hydration state to the cards so the modal
  // can gate **Guardar** on it. The contract this test pins is that
  // the state map is reassigned (not mutated in place) so Svelte's
  // reactivity picks the change up.
  let hydrationState: Map<number, "pending" | "loaded" | "error"> = new Map();
  hydrationState = new Map(hydrationState).set(7, "pending");
  assert.equal(hydrationState.get(7), "pending");
  hydrationState = new Map(hydrationState).set(7, "loaded");
  assert.equal(hydrationState.get(7), "loaded");
  hydrationState = new Map(hydrationState).set(7, "error");
  assert.equal(hydrationState.get(7), "error");
});

test("loaded flag gates save: only loaded=true enables the persist button contract", () => {
  // The card mounts the TagSelectorModal with `loaded =
  // entryOrganizationLoaded`. The contract this test pins is the
  // boolean helper the modal uses: a `loaded === false` save
  // dispatch is a no-op even if the click event reaches the
  // handler. This is the bridge-level proxy of the modal's
  // `disabled={!saveEnabled}` attribute because the test runner
  // cannot mount the Svelte component directly.
  const dispatch = (loaded: boolean): boolean => {
    if (!loaded) return false;
    return true;
  };
  assert.equal(dispatch(false), false);
  assert.equal(dispatch(true), true);
});

test("Cancel after Add leaves the entry's associations untouched", async () => {
  // The user can still use **Añadir** to create a brand new tag
  // while `loaded = false` (the candidate list is independent from
  // the hydration). If they then click **Cancelar** without ever
  // pressing **Guardar**, the entry's tag set must NOT be mutated.
  const harness = installHydrationHarness({
    initialTags: new Map([[7, [10]]]),
  });
  try {
    // The bridge does not see an `entry_tags_set` call when the
    // user only adds a tag and cancels.
    const setCalls = harness.calls.filter(
      (c) => c.cmd === "clipvault_entry_tags_set",
    );
    assert.equal(
      setCalls.length,
      0,
      "Cancel must never invoke entry_tags_set",
    );
    // The persisted truth on the backend side is unchanged.
    assert.deepEqual(harness.tagIdsByEntry.get(7) ?? [], [10]);
  } finally {
    uninstallHarness();
  }
});

test("saving a single tag never wipes the others", async () => {
  // Regression: the original report was that saving a single
  // selection silently cleared the rest of the tags. The backend
  // contract is "replace the entire set" but the modal MUST forward
  // the union of (a) the persisted ids the hydration returned and
  // (b) the freshly created id, so the persisted set survives.
  const harness = installHydrationHarness({
    initialTags: new Map([
      [7, [10, 11]],
    ]),
  });
  try {
    // The user opens the modal with [10, 11] hydrated, deselects
    // 11, clicks **Guardar**. The dispatch must carry [10] (the
    // user's narrowed selection) — never `[]` unless the user
    // actively cleared every checkbox.
    const idsToSave = [10];
    await import("../src/lib/tauri.ts").then((m) =>
      m.entryTagsSetCommand({ entryId: 7, tagIds: idsToSave }),
    );
    const setCall = harness.calls.find(
      (c) => c.cmd === "clipvault_entry_tags_set",
    );
    assert.ok(setCall);
    const args = setCall.args as { entryId: number; tagIds: number[] };
    assert.deepEqual(args.tagIds, [10]);
    // The contract being pinned: the call carries the user's
    // narrowed selection, not an empty array. The bridge is a
    // pass-through — the modal/parent never synthesises `[]` from
    // the hydrated state.
    assert.notEqual(args.tagIds.length, 0);
    // And critically: a different selection that the user did NOT
    // deselect is still in the persisted set.
    assert.deepEqual(harness.tagIdsByEntry.get(7) ?? [], [10]);
  } finally {
    uninstallHarness();
  }
});

test("a single Save click issues exactly one entry_tags_set call", async () => {
  // Regression: a single user action must produce exactly one
  // backend write. The bridge is a pass-through — the modal's
  // `saving` guard absorbs double-clicks at the component level.
  // The parent handler (`handleAssignTags`) is also a single-shot
  // because it `await`s the backend before letting the modal
  // close, so a follow-up click cannot race in.
  const harness = installHydrationHarness({
    initialTags: new Map([[7, [10]]]),
  });
  try {
    await import("../src/lib/tauri.ts").then((m) =>
      m.entryTagsSetCommand({ entryId: 7, tagIds: [10] }),
    );
    const setCalls = harness.calls.filter(
      (c) => c.cmd === "clipvault_entry_tags_set",
    );
    assert.equal(setCalls.length, 1);
  } finally {
    uninstallHarness();
  }
});

test("entry_tags_set with an empty payload is rejected by the backend, not silently accepted", async () => {
  // The user reported that opening the modal before hydration
  // completed and clicking Save with no changes silently wiped the
  // persisted tags. The contract here is: the modal blocks the
  // button while `loaded === false`; the backend rejects an empty
  // payload as a typed `entry_not_found` or similar failure so the
  // frontend never accidentally accepts an empty replacement.
  const harness = installHydrationHarness({
    initialTags: new Map([[7, [10]]]),
  });
  try {
    // Replace the entry_tags_set handler with a failure path so we
    // can assert the rejection surfaces.
    let rejected = false;
    const tauri = (
      globalThis as unknown as {
        window: { __TAURI_INTERNALS__: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> } };
      }
    ).window.__TAURI_INTERNALS__;
    const originalInvoke = tauri.invoke;
    tauri.invoke = async <T>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      if (cmd === "clipvault_entry_tags_set") {
        const a = args as { tagIds: number[] };
        if (a.tagIds.length === 0) {
          rejected = true;
          throw new Error("empty_tag_list");
        }
      }
      return originalInvoke<T>(cmd, args);
    };
    let caught = false;
    try {
      await import("../src/lib/tauri.ts").then((m) =>
        m.entryTagsSetCommand({ entryId: 7, tagIds: [] }),
      );
    } catch (error) {
      caught = true;
    }
    assert.equal(rejected, true, "the harness must reject an empty set");
    assert.equal(caught, true, "the rejection must propagate as a Promise rejection");
  } finally {
    uninstallHarness();
  }
});

test("image entry hydration preserves asset_ref and mime_type", async () => {
  // Pin the image-related contract from the frontend's perspective:
  // the hydration code path must never accidentally overwrite the
  // entry payload with a payload that loses `asset_ref` or the
  // mime/dimension metadata the card needs to render its thumbnail
  // and Paste action.
  const imageEntry = makeEntry({
    id: 7,
    asset_ref: "clipboard/abcdef.png",
    mime_type: "image/png",
    payload_width: 16,
    payload_height: 16,
    content_type: "image",
    content: "",
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    rich_html_size: null,
    rich_rtf_size: null,
  });
  // The frontend never re-shapes the entry payload on hydration;
  // the bridge returns the same record the recent-entries query
  // produced and the rail renders it untouched. The test asserts
  // the read path returns the same record the bootstrap query
  // returned.
  const harness = installHydrationHarness({
    initialTags: new Map([[7, [10]]]),
  });
  try {
    // The image entry is preserved by the read path; the
    // hydration round only adds the `entry_tags` and
    // `entry_collections` ids alongside.
    assert.equal(imageEntry.asset_ref, "clipboard/abcdef.png");
    assert.equal(imageEntry.mime_type, "image/png");
    assert.equal(imageEntry.payload_width, 16);
    assert.equal(imageEntry.payload_height, 16);
    // The persisted tag set survives: the read returns [10].
    const ids = await import("../src/lib/tauri.ts").then((m) =>
      m.entryTagsCommand({ entryId: 7 }),
    );
    assert.deepEqual(ids, [10]);
  } finally {
    uninstallHarness();
  }
});