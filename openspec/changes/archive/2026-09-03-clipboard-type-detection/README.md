# clipboard-type-detection

This change introduces a deterministic, conservative content-type detector
for textual clipboard captures. The detector lives in `clipvault-core`,
runs without Tauri, SQLite, the clipboard or the network, and stores the
classification in the existing `content_type` column of
`clipboard_entries`.

Existing rows keep their `content_type = 'text'` value; the change only
adds new variants and widens the SQL filter that powers local search so
classified entries keep surfacing alongside the legacy text rows.
