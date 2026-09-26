/** Validation and capability rules for source-app names on remote rows. */
export const MAX_REMOTE_SOURCE_APP_NAME_CHARS = 128;

export function supportsRemoteSourceAppPresentation(
  capability: string | null,
): boolean {
  return (
    capability
      ?.split(",")
      .some((token) => token.trim() === "source_app_presentation") ?? false
  );
}

/** Reject malformed host names; never silently strip control characters. */
export function validatedRemoteSourceAppName(
  input: string | null,
): string | null {
  if (input === null) return null;
  const name = input.trim();
  if (
    name.length === 0 ||
    Array.from(name).length > MAX_REMOTE_SOURCE_APP_NAME_CHARS ||
    /[\u0000-\u001f\u007f-\u009f]/u.test(name)
  ) {
    return null;
  }
  return name;
}
