## Why

La blacklist actual exige introducir a mano identificadores de plataforma como
com.apple.Terminal. Esto es poco descubrible, propenso a errores y obliga al
usuario a conocer detalles de macOS que la aplicación ya puede resolver desde
el bundle instalado.

La experiencia debe permitir seleccionar visualmente una aplicación y mostrar
en la lista una representación reconocible: icono y nombre. El identificador
debe seguir existiendo internamente porque es el contrato estable que utiliza
el PrivacyGate.

## What Changes

- Reemplazar el input de texto de la blacklist por un botón Seleccionar aplicación.
- Abrir un selector nativo de aplicaciones, inicialmente en /Applications en macOS.
- Validar que la selección sea un bundle .app permitido y no ejecutar la aplicación.
- Extraer del bundle el identificador, nombre visible e icono.
- Persistir la metadata necesaria para renderizar la lista después de reiniciar.
- Mostrar filas con icono y nombre, con fallback seguro para registros antiguos.
- Mantener el matching existente, la captura, la búsqueda, quick-paste y la privacidad.

## Capabilities

### Modified Capabilities

- privacy-settings: la blacklist se administra mediante selección visual y conserva el identificador como dato interno.

### New Platform Surface

- Selector de aplicación nativo y extractor de metadata, encapsulado detrás de un trait sustituible por fakes.

## Non-Goals

- No se rediseña PrivacyGate ni el refresco de active app.
- No se agrega un catálogo global de aplicaciones.
- No se ejecuta, instala, abre ni modifica la aplicación seleccionada.
- No se implementa todavía un selector Linux si no existe una correspondencia segura entre .desktop y WM_CLASS/app id.
- No se agregan favoritos, tags, colecciones, imágenes de clipboard ni red.
- No se registran rutas arbitrarias, contenido del clipboard ni telemetría.
