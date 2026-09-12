# Aliases de metadata e iconos de paquetes Linux

## ADDED Requirements

### Requirement: Alias ejecutable local y exacto

Cuando un identificador Linux sin sufijo `.desktop` no coincide con
`StartupWMClass`, `X-GNOME-WMClass` ni el filename stem, el provider SHALL
considerar el basename exacto y seguro del primer token de `Exec=` de la
entrada `.desktop`. MUST NOT ejecutar `Exec=`, expandir shell, consultar PID o
aceptar prefijos, substrings o similitudes. `source_app` SHALL permanecer
intacto.

#### Scenario: xterm con entrada prefijada por la distribución

- GIVEN una entrada `debian-xterm.desktop` con `Type=Application`,
  `Exec=xterm` e `Icon=mini.xterm`
- AND la captura permitida tiene `source_app = "xterm"`
- AND `mini.xterm` existe en una raíz XDG de iconos
- WHEN el provider resuelve metadata
- THEN conserva `source_app = "xterm"`
- AND selecciona `MatchStrategy::ExecBasename`
- AND persiste el nombre y un PNG en `application-icons/`
- AND no ejecuta ni transporta el valor de `Exec=`

#### Scenario: alias no parcial

- GIVEN una entrada con `Exec=xterm`
- WHEN el provider recibe `xterm-extra`
- THEN no usa esa entrada por prefijo o substring
- AND conserva el fallback actual sin crear un asset

### Requirement: Iconos absolutos de paquetes locales

Cuando una entrada local declara `Icon=` como ruta absoluta, el provider SHALL
resolverla sólo si el archivo canonicalizado queda dentro de una raíz local
de iconos o de payload de paquete explícitamente permitida por el adaptador
Linux. MUST validar archivo regular, formato y límites existentes; MUST
rechazar symlinks que escapen y no SHALL permitir `/`, `/tmp` ni el home
completo como raíz implícita.

#### Scenario: Firefox Snap con icono en el payload

- GIVEN `firefox_firefox.desktop` existe en una raíz de aplicaciones local
- AND declara `Icon=/snap/firefox/current/default256.png`
- AND ese PNG queda dentro de una raíz Snap permitida
- WHEN la captura permitida tiene `source_app = "firefox_firefox.desktop"`
- THEN el match es `desktop_file_id`
- AND el provider persiste `source_app_name = "Firefox"`
- AND persiste un `source_app_icon_ref` relativo bajo `application-icons/`
- AND el diagnóstico no expone la ruta absoluta ni bytes

#### Scenario: Ruta absoluta fuera de allowlist

- GIVEN una entrada que declara un icono absoluto bajo `/tmp` o mediante un
  symlink que escapa de la raíz permitida
- WHEN el provider resuelve metadata
- THEN puede conservar el nombre de la aplicación
- AND no persiste ni reemplaza el asset existente
- AND registra sólo `out_of_roots` como categoría estable

### Requirement: Precedencia y compatibilidad existentes

La incorporación de aliases y raíces de paquetes SHALL conservar la
precedencia XDG y los matchers existentes. Un Desktop File ID con sufijo
`.desktop` MUST continuar siendo estricto y no caer a aliases cuando no haya
match exacto.

#### Scenario: WM_CLASS gana sobre Exec

- GIVEN una entrada con `StartupWMClass=terminal` y `Exec=terminal-wrapper`
- WHEN el provider recibe `terminal`
- THEN usa `startup_wm_class`
- AND no cambia la identidad persistida ni el asset key

#### Scenario: Desktop File ID no se degrada

- GIVEN una entrada `firefox.desktop` con `Exec=firefox`
- WHEN el provider recibe `other-firefox.desktop`
- THEN no usa `Exec=firefox` ni filename stem como fallback
- AND devuelve el resultado no resoluble actual
