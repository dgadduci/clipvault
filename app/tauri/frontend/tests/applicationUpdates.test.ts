import { test } from "node:test";
import assert from "node:assert/strict";
import { get } from "svelte/store";
import {
  createUpdateController,
  supportsApplicationUpdates,
  type UpdateDriver,
  type UpdatePackage,
} from "../src/lib/applicationUpdates.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function makePackage(version = "0.0.16"): UpdatePackage {
  return {
    version,
    async downloadAndInstall() {},
  };
}

test("application updates are enabled only in production Tauri builds", () => {
  assert.equal(supportsApplicationUpdates(true, true), true);
  assert.equal(supportsApplicationUpdates(false, true), false);
  assert.equal(supportsApplicationUpdates(true, false), false);
});

test("automatic checks are single-flight and expose a compatible update", async () => {
  const result = deferred<UpdatePackage | null>();
  let checks = 0;
  const driver: UpdateDriver = {
    check: () => {
      checks += 1;
      return result.promise;
    },
    async relaunch() {},
  };
  const controller = createUpdateController(driver, () => true);

  controller.startAutomaticCheck();
  controller.startAutomaticCheck();
  const manualCheck = controller.checkForUpdates();
  assert.equal(checks, 1);
  assert.equal(get(controller.state).status, "checking");

  result.resolve(makePackage());
  await manualCheck;
  assert.deepEqual(get(controller.state), {
    status: "available",
    availableVersion: "0.0.16",
    progressPercent: null,
    errorPhase: null,
  });
});

test("failed network checks remain recoverable through a manual retry", async () => {
  let checks = 0;
  const driver: UpdateDriver = {
    async check() {
      checks += 1;
      if (checks === 1) throw new Error("offline");
      return makePackage();
    },
    async relaunch() {},
  };
  const controller = createUpdateController(driver, () => true);

  await controller.checkForUpdates();
  assert.equal(get(controller.state).errorPhase, "check");
  await controller.checkForUpdates();
  assert.equal(get(controller.state).status, "available");
  assert.equal(checks, 2);
});

test("installation reports progress and only then enables relaunch", async () => {
  const installation = deferred<void>();
  let progressHandler: ((event: {
    event: "Started" | "Progress" | "Finished";
    data?: { contentLength?: number; chunkLength?: number };
  }) => void) | null = null;
  let relaunched = false;
  const update: UpdatePackage = {
    version: "0.0.16",
    downloadAndInstall: (onEvent) => {
      progressHandler = onEvent;
      return installation.promise;
    },
  };
  const driver: UpdateDriver = {
    async check() {
      return update;
    },
    async relaunch() {
      relaunched = true;
    },
  };
  const controller = createUpdateController(driver, () => true);

  await controller.checkForUpdates();
  const installing = controller.installAvailableUpdate();
  assert.ok(progressHandler);
  progressHandler({ event: "Started", data: { contentLength: 10 } });
  progressHandler({ event: "Progress", data: { chunkLength: 4 } });
  assert.equal(get(controller.state).status, "installing");
  assert.equal(get(controller.state).progressPercent, 40);

  progressHandler({ event: "Finished" });
  installation.resolve();
  await installing;
  assert.equal(get(controller.state).status, "restart_required");
  assert.equal(relaunched, false);

  await controller.restartAfterUpdate();
  assert.equal(relaunched, true);
});

test("a rejected or failed installation leaves the current client usable", async () => {
  let checks = 0;
  let relaunched = false;
  const update = makePackage();
  update.downloadAndInstall = async () => {
    throw new Error("signature or installation rejected");
  };
  const driver: UpdateDriver = {
    async check() {
      checks += 1;
      return update;
    },
    async relaunch() {
      relaunched = true;
    },
  };
  const controller = createUpdateController(driver, () => true);

  await controller.checkForUpdates();
  await controller.installAvailableUpdate();
  assert.deepEqual(get(controller.state), {
    status: "error",
    availableVersion: "0.0.16",
    progressPercent: null,
    errorPhase: "install",
  });
  assert.equal(relaunched, false);

  await controller.checkForUpdates();
  assert.equal(get(controller.state).status, "available");
  assert.equal(checks, 2);
});

test("disabled development builds do not query the updater", async () => {
  let checks = 0;
  const controller = createUpdateController({
    async check() {
      checks += 1;
      return null;
    },
    async relaunch() {},
  }, () => false);

  controller.startAutomaticCheck();
  await controller.checkForUpdates();
  assert.equal(checks, 0);
  assert.equal(get(controller.state).status, "idle");
});
