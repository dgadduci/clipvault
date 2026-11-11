## 1. Collection row presentation

- [x] 1.1 Render the assigned collection color as a validated 18%-opacity row
  background while retaining selected and drag-over feedback.
- [x] 1.2 Keep each row's name, applicable `system`/`imported` badge, capture
  count, note icon and one actions-menu trigger; remove the separate color and
  delete controls.

## 2. Actions menu and accessibility

- [x] 2.1 Keep the color action and user-only deletion in the per-row menu;
  place the note icon between the count badge and menu trigger.
- [x] 2.2 Preserve the shared note modal with focus returning to its row icon,
  parent-owned deletion confirmation, selection and drop flows.
- [x] 2.3 Support localized accessible naming, keyboard menu navigation,
  Escape and outside-click dismissal, and focus restoration.
- [x] 2.4 Add the menu-trigger label in all five locale catalogs and confirm key
  and placeholder parity.

## 3. Regression coverage and verification

- [x] 3.1 Update focused sidebar/menu regressions for the new row structure,
  tinted backgrounds, system/imported badges and available actions.
- [ ] 3.2 Run frontend collection and drag-and-drop regressions, locale
  validation, frontend check and build.
- [ ] 3.3 Validate this change with strict OpenSpec validation.
- [x] 3.4 Review the complete diff for scope, warnings and generated or
  sensitive files.

La validación de locales y la revisión del diff se completaron. Se intentaron
las regresiones frontend, el check/build y `openspec validate`, incluso fuera
del sandbox; todos siguen pendientes porque el entorno bloquea Node/npm y el
CLI de OpenSpec (`operation not permitted`).
