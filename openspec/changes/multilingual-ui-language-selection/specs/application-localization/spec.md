## ADDED Requirements

### Requirement: Complete local translation catalogs

The application SHALL provide complete JSON translation catalogs for English,
Spanish, Portuguese, German, and French. The catalogs SHALL use the same
namespaced keys and SHALL be the source for all ClipVault-authored
user-facing text. This includes the main window, QuickVault, dialogs,
settings, menus, system tray labels, status and error messages, tooltips, and
accessible labels. User-created content, external application names, and
operating-system-provided text are not application-authored strings.

#### Scenario: Catalogs are checked for completeness

- **WHEN** localization catalogs are validated during development or build
- **THEN** every supported language has the same set of keys
- **AND** every required translation is non-empty

#### Scenario: A translation key is unavailable at runtime

- **WHEN** a requested key is missing from the selected catalog
- **THEN** the application displays the English catalog value for that key
- **AND** the missing key remains detectable by catalog validation

### Requirement: Select and persist the interface language

The application SHALL provide a language selector in General Settings with
English, Spanish, Portuguese, German, and French as available choices. The
selected locale SHALL be stored in the existing local application settings
and SHALL accept only en, es, pt, de, or fr.

#### Scenario: A user selects a language

- **WHEN** the user selects a supported language in General Settings
- **THEN** the preference is saved locally
- **AND** the selected choice is displayed using the language's own name

#### Scenario: The saved locale is absent or unsupported

- **WHEN** the application loads settings with no locale or an unsupported
  locale value
- **THEN** the application selects English
- **AND** it continues to operate without requiring a database migration

### Requirement: Use English as the default language

The application SHALL use English for a new installation unless the user
selects another supported language. It SHALL NOT select a locale by detecting
the operating-system language.

#### Scenario: First launch without a saved language

- **WHEN** the application starts without a saved locale preference
- **THEN** all ClipVault-authored interface text is shown in English

### Requirement: Apply language changes immediately

The application SHALL apply a successfully saved locale change during the
current session without requiring an application restart. The change SHALL
reach all open ClipVault windows and update native ClipVault menus that can be
rebuilt at runtime.

#### Scenario: Language changes while the main window is open

- **WHEN** the user selects and saves another supported language
- **THEN** visible and subsequently opened interface surfaces use that
  language immediately
- **AND** the open main window and QuickVault window use the same locale

#### Scenario: Language preference cannot be saved

- **WHEN** persisting a newly selected locale fails
- **THEN** the active locale remains unchanged
- **AND** the application presents the failure in the active locale

### Requirement: Format dynamic localized text

The application SHALL represent dynamic interface text with named
placeholders and use locale-aware plural, number, and date formatting where
applicable. It SHALL NOT build user-facing translations by concatenating
translated sentence fragments.

#### Scenario: A translated message includes dynamic values

- **WHEN** a localized message includes a count, date, number, or other value
- **THEN** the value is formatted according to the active locale
- **AND** all supported catalogs retain the same required placeholder names

### Requirement: Keep localization offline

The application SHALL load translations from packaged local JSON catalogs and
SHALL NOT require network access or a remote translation service to display
any supported language.

#### Scenario: The application runs without network access

- **WHEN** the application starts or the user changes its language while
  offline
- **THEN** all supported translations remain available
