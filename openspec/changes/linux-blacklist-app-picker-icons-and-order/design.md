# Diseño: selector Linux ordenado con iconos locales

## Decisiones

### 1. El catálogo define el orden canónico

`LinuxApplicationCatalog::list()` seguirá siendo el único lugar que define el
orden de las candidatas. La interfaz no debe volver a ordenar la colección con
un criterio diferente.

La clave de orden será, en este orden:

1. `display_name.trim()` cuando exista y no esté vacío;
2. `identifier` como fallback cuando no haya un nombre visible;
3. comparación case-insensitive y locale-independent usando la biblioteca
   estándar;
4. el identificador normalizado y luego el identificador original como
   desempates deterministas.

La implementación no dependerá del orden de descubrimiento del sistema ni de
la estrategia usada para encontrar la aplicación (`WmClass` o
`DesktopFileId`). Los nombres conservan su forma original para presentarse en
la UI; la normalización solo se usa para comparar.

Se deben reemplazar las pruebas que expresan orden por `identifier` por casos
que demuestren orden por nombre visible, fallback a identificador, diferencias
de mayúsculas y desempates estables.

### 2. El selector reutiliza el bridge correcto de iconos

El proveedor Linux ya resuelve metadatos locales mediante las rutas XDG,
persiste los iconos rasterizados bajo `application-icons/` y entrega un
`icon_ref` relativo. El selector debe reutilizar ese contrato:

```text
CandidateApplication.icon_ref
        -> sourceAppIconCommand({ ref })
        -> createIconResolver(...)
        -> Blob URL local
        -> <img>
```

`sourceAppIconCommand` y `clipvault_source_app_icon` son el bridge existente
para el namespace `application-icons/`, usado también por las capturas de
Wayland. No se debe agregar un comando paralelo ni hacer que el selector llame
`ignoredAppIconCommand`, porque ese comando solo acepta referencias del
namespace `ignored-apps/`.

La UI mantendrá el fallback actual a la inicial para referencias ausentes,
fallidas o no resolubles. La resolución de una fila no debe impedir que las
demás se rendericen. Las Blob URLs se liberan al cerrar el modal, al destruir
el componente y al reemplazar o descartar una resolución, siguiendo el ciclo
de vida del resolver existente.

No se permitirán accesos directos del frontend al sistema de archivos, rutas
absolutas ni URLs remotas. El backend seguirá validando referencias relativas,
el namespace permitido y el formato de imagen.

### 3. Compatibilidad con X11 y Wayland

Para X11/XWayland se conservará la metadata que ya obtiene el catálogo desde
las aplicaciones observadas. Para Wayland se conservará el camino basado en
Desktop File ID: `Name` e `Icon` se resuelven a través del proveedor XDG local,
incluida la rasterización SVG local ya existente cuando corresponde.

No se incorporarán `Shell.App.get_icon()`, `Gio.Icon`, títulos de ventana, PID,
`/proc` ni cambios en la extensión GNOME Shell. Las candidatas respaldadas
únicamente por una ventana (`window:*`) pueden continuar sin icono y usar el
fallback.

### 4. Semántica de blacklist sin cambios

El cambio es exclusivamente de presentación y orden. El alta, la baja, la
cancelación, `activation_pending`, la persistencia y la evaluación de
`PrivacyGate` deben conservar sus contratos actuales.

El selector seguirá enviando únicamente el identificador opaco de la
candidata. No debe enviar nombre, contenido, hash, bytes de imagen, rutas ni
otros datos de la aplicación. Si durante la implementación aparece una
contradicción entre el namespace persistido de una entrada ignorada y el
`icon_ref` de una candidata, se debe pausar y actualizar OpenSpec; no se debe
inventar una copia o conversión de assets dentro de este cambio.

## Archivos y áreas probables

- `crates/clipvault-platform/src/runtime/linux_app_catalog.rs`: clave de orden
  y pruebas del catálogo.
- `app/tauri/frontend/src/PrivacyModal.svelte`: bridge usado para cargar
  iconos de candidatas y ciclo de vida del resolver.
- `app/tauri/frontend/src/lib/tauri.ts`: solo si hace falta ajustar el uso del
  comando ya existente; no crear un bridge nuevo.
- Tests existentes del catálogo, del resolver frontend y de
  `clipvault_source_app_icon`, agregando únicamente cobertura faltante.

## Verificación

La validación debe incluir formato Rust, tests dirigidos del catálogo y del
bridge, checks/tests/build del frontend, `openspec validate --strict` y
`git diff --check`. La prueba manual final debe ejecutarse con la instancia
anterior de ClipVault cerrada, primero en X11 y luego en Wayland, comprobando
orden, iconos, fallback y que la blacklist siga bloqueando capturas.

No se requiere ejecutar regresiones de drag and drop porque este cambio no
modifica cards, colecciones, layout ni listeners de interacción.
