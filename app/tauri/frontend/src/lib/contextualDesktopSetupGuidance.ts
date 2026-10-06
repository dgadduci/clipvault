import type {
  ContextualDesktopSetupDismissals,
  DesktopSetupIntegration,
  Diagnostics,
  GnomeIntegrationStatusResponse,
  KdeKwinIntegrationPayload,
} from "../types";

/**
 * Select only integrations confirmed by their typed status responses. Desktop
 * and display-server diagnostics gate the lookup; free-form distribution or
 * desktop names never decide which integration is offered.
 */
export function contextualDesktopSetupCandidates(
  diagnostics: Pick<Diagnostics, "platform_os" | "display_server">,
  gnome: GnomeIntegrationStatusResponse | null,
  kde: KdeKwinIntegrationPayload | null,
  dismissals: ContextualDesktopSetupDismissals,
): DesktopSetupIntegration[] {
  if (
    diagnostics.platform_os !== "linux" ||
    diagnostics.display_server !== "wayland"
  ) {
    return [];
  }

  const candidates: DesktopSetupIntegration[] = [];
  if (
    !dismissals.gnome_dismissed &&
    gnome?.kind === "ready" &&
    gnome.payload.applicable &&
    gnome.payload.consent === "unknown" &&
    gnome.payload.technical_state === "not_installed"
  ) {
    candidates.push("gnome");
  }

  if (
    !dismissals.kde_dismissed &&
    kde?.applicable === true &&
    kde.consent === "unknown" &&
    (kde.technical_state === "awaiting_consent" ||
      kde.technical_state === "not_installed") &&
    !kde.enabled
  ) {
    candidates.push("kde");
  }

  return candidates;
}
