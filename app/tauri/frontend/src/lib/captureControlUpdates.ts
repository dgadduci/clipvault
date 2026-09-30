import { listen } from "@tauri-apps/api/event";

export const CAPTURE_CONTROL_CHANGED_EVENT =
  "clipvault://capture-control-changed";
export const CAPTURE_CONTROL_ERROR_EVENT =
  "clipvault://capture-control-error";

export interface CaptureControlChangedPayload {
  enabled: boolean;
}

export interface CaptureControlErrorPayload {
  kind: "persistence_failed";
}

export function listenCaptureControlChanged(
  handler: (enabled: boolean) => void,
): Promise<() => void> {
  return listen<CaptureControlChangedPayload>(
    CAPTURE_CONTROL_CHANGED_EVENT,
    (event) => handler(event.payload.enabled),
  );
}

export function listenCaptureControlError(
  handler: (kind: CaptureControlErrorPayload["kind"]) => void,
): Promise<() => void> {
  return listen<CaptureControlErrorPayload>(
    CAPTURE_CONTROL_ERROR_EVENT,
    (event) => handler(event.payload.kind),
  );
}
