import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { get, writable, type Writable } from "svelte/store";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "installing"
  | "restart_required"
  | "error";

export type UpdateErrorPhase = "check" | "install" | "restart";

export interface UpdateState {
  status: UpdateStatus;
  availableVersion: string | null;
  progressPercent: number | null;
  errorPhase: UpdateErrorPhase | null;
}

export interface UpdatePackage {
  version: string;
  downloadAndInstall(onEvent: (event: DownloadEvent) => void): Promise<void>;
  close?(): Promise<void>;
}

export interface UpdateDriver {
  check(): Promise<UpdatePackage | null>;
  relaunch(): Promise<void>;
}

export interface UpdateController {
  state: Writable<UpdateState>;
  checkForUpdates(): Promise<void>;
  installAvailableUpdate(): Promise<void>;
  restartAfterUpdate(): Promise<void>;
  startAutomaticCheck(): void;
}

const initialState: UpdateState = {
  status: "idle",
  availableVersion: null,
  progressPercent: null,
  errorPhase: null,
};

const tauriUpdateDriver: UpdateDriver = {
  async check(): Promise<Update | null> {
    return check();
  },
  relaunch,
};

export function createUpdateController(
  driver: UpdateDriver,
  isEnabled: () => boolean,
): UpdateController {
  const state = writable<UpdateState>({ ...initialState });
  let pendingUpdate: UpdatePackage | null = null;
  let checkInFlight: Promise<void> | null = null;
  let installInFlight: Promise<void> | null = null;
  let automaticCheckStarted = false;

  async function closeUpdate(update: UpdatePackage | null): Promise<void> {
    if (!update?.close) return;
    try {
      await update.close();
    } catch {
      // Releasing an updater resource must not replace the user's result.
    }
  }

  function checkForUpdates(): Promise<void> {
    if (!isEnabled()) return Promise.resolve();
    if (checkInFlight) return checkInFlight;
    const currentState = get(state);
    if (currentState.status === "installing" || currentState.status === "restart_required") {
      return Promise.resolve();
    }

    const previousUpdate = pendingUpdate;
    pendingUpdate = null;
    void closeUpdate(previousUpdate);
    state.set({ ...initialState, status: "checking" });

    const task = (async () => {
      try {
        const update = await driver.check();
        pendingUpdate = update;
        state.set(update
          ? {
              status: "available",
              availableVersion: update.version,
              progressPercent: null,
              errorPhase: null,
            }
          : { ...initialState, status: "current" });
      } catch {
        state.set({
          status: "error",
          availableVersion: null,
          progressPercent: null,
          errorPhase: "check",
        });
      }
    })();
    checkInFlight = task;
    void task.finally(() => {
      if (checkInFlight === task) checkInFlight = null;
    });
    return task;
  }

  function installAvailableUpdate(): Promise<void> {
    if (!isEnabled()) return Promise.resolve();
    if (installInFlight) return installInFlight;
    const update = pendingUpdate;
    if (!update) return checkForUpdates();

    let expectedBytes: number | null = null;
    let downloadedBytes = 0;
    state.set({
      status: "installing",
      availableVersion: update.version,
      progressPercent: null,
      errorPhase: null,
    });

    const task = (async () => {
      try {
        await update.downloadAndInstall((event) => {
          if (event.event === "Started") {
            expectedBytes = event.data.contentLength ?? null;
            downloadedBytes = 0;
            state.update((current) => ({
              ...current,
              progressPercent: expectedBytes && expectedBytes > 0 ? 0 : null,
            }));
          } else if (event.event === "Progress") {
            downloadedBytes += event.data.chunkLength;
            state.update((current) => ({
              ...current,
              progressPercent: expectedBytes && expectedBytes > 0
                ? Math.min(99, Math.floor((downloadedBytes / expectedBytes) * 100))
                : null,
            }));
          } else {
            state.update((current) => ({ ...current, progressPercent: 100 }));
          }
        });
        pendingUpdate = null;
        await closeUpdate(update);
        state.set({
          status: "restart_required",
          availableVersion: update.version,
          progressPercent: null,
          errorPhase: null,
        });
      } catch {
        pendingUpdate = null;
        await closeUpdate(update);
        state.set({
          status: "error",
          availableVersion: update.version,
          progressPercent: null,
          errorPhase: "install",
        });
      }
    })();
    installInFlight = task;
    void task.finally(() => {
      if (installInFlight === task) installInFlight = null;
    });
    return task;
  }

  async function restartAfterUpdate(): Promise<void> {
    const currentState = get(state);
    if (!isEnabled() || currentState.status !== "restart_required" &&
      !(currentState.status === "error" && currentState.errorPhase === "restart")) {
      return;
    }

    state.set({
      ...currentState,
      status: "restart_required",
      errorPhase: null,
    });
    try {
      await driver.relaunch();
    } catch {
      state.set({
        ...currentState,
        status: "error",
        errorPhase: "restart",
      });
    }
  }

  function startAutomaticCheck(): void {
    if (automaticCheckStarted) return;
    automaticCheckStarted = true;
    void checkForUpdates();
  }

  return {
    state,
    checkForUpdates,
    installAvailableUpdate,
    restartAfterUpdate,
    startAutomaticCheck,
  };
}

export function isApplicationUpdaterEnabled(): boolean {
  return import.meta.env.PROD &&
    import.meta.env.VITE_CLIPVAULT_UPDATER_ENABLED === "true" &&
    isTauri();
}

const applicationUpdateController = createUpdateController(
  tauriUpdateDriver,
  isApplicationUpdaterEnabled,
);

export const updateState = applicationUpdateController.state;
export const checkForUpdates = applicationUpdateController.checkForUpdates;
export const installAvailableUpdate = applicationUpdateController.installAvailableUpdate;
export const restartAfterUpdate = applicationUpdateController.restartAfterUpdate;
export const startAutomaticUpdateCheck = applicationUpdateController.startAutomaticCheck;
