export interface StartupStatus {
  status: "pending" | "ready" | "failed";
  language: string;
}

export const MINIMUM_SPLASH_DURATION_MS = 3000;

type TimerScheduler = (callback: () => void, delayMs: number) => unknown;
type TimerCanceller = (timer: unknown) => void;

export function scheduleMinimumSplashDuration(
  onElapsed: () => void,
  schedule: TimerScheduler = (callback, delayMs) => setTimeout(callback, delayMs),
  cancel: TimerCanceller = (timer) =>
    clearTimeout(timer as ReturnType<typeof setTimeout>),
): () => void {
  const timer = schedule(onElapsed, MINIMUM_SPLASH_DURATION_MS);
  return () => cancel(timer);
}

export function canRevealStartupDesktop(
  minimumElapsed: boolean,
  startupSettled: boolean,
): boolean {
  return minimumElapsed && startupSettled;
}

export async function waitForStartupStatus(
  readStatus: () => Promise<StartupStatus>,
  intervalMs = 120,
): Promise<StartupStatus> {
  for (;;) {
    const snapshot = await readStatus();
    if (snapshot.status !== "pending") return snapshot;
    await new Promise<void>((resolve) => setTimeout(resolve, intervalMs));
  }
}
