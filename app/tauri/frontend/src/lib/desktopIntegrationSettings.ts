import type {
  GnomeIntegrationStatusResponse,
  KdeKwinIntegrationPayload,
} from "../types";

export type DesktopIntegrationStatusKind =
  | "setup"
  | "active"
  | "pending"
  | "disconnected"
  | "disabled"
  | "declined"
  | "unsupported";

/** Whether the detected session has a GNOME or KDE integration to manage. */
export function hasManageableDesktopIntegration(
  gnome: GnomeIntegrationStatusResponse | null,
  kde: KdeKwinIntegrationPayload | null,
): boolean {
  const gnomeAvailable =
    gnome?.kind === "ready" && gnome.payload.applicable;
  const kdeAvailable = kde?.applicable === true;
  return gnomeAvailable || kdeAvailable;
}

export function gnomeIntegrationStatusKind(
  status: GnomeIntegrationStatusResponse,
): DesktopIntegrationStatusKind {
  if (status.kind !== "ready") return "unsupported";
  const payload = status.payload;
  if (!payload.applicable) return "unsupported";
  if (payload.consent === "declined") return "declined";
  if (payload.consent === "disabled" || payload.technical_state === "disabled") {
    return "disabled";
  }

  switch (payload.technical_state) {
    case "identified":
    case "no_active_application":
    case "connected":
      return "active";
    case "activation_pending":
      return "pending";
    case "disconnected":
    case "communication_error":
      return "disconnected";
    case "incompatible":
    case "unavailable":
      return "unsupported";
    case "not_installed":
    default:
      return "setup";
  }
}

export function kdeIntegrationStatusKind(
  status: KdeKwinIntegrationPayload,
): DesktopIntegrationStatusKind {
  if (!status.applicable) return "unsupported";
  if (status.consent === "declined") return "declined";
  if (status.consent === "disabled" || status.technical_state === "disabled") {
    return "disabled";
  }

  switch (status.technical_state) {
    case "identified":
    case "no_active_application":
      return "active";
    case "activation_pending":
      return "pending";
    case "disconnected":
    case "communication_error":
      return "disconnected";
    case "not_installed":
    case "awaiting_consent":
      return "setup";
    case "not_applicable":
    default:
      return status.enabled ? "active" : "unsupported";
  }
}
