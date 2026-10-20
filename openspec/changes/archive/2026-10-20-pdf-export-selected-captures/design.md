# Design: pdf-export-selected-captures

## Flujo

1. Desktop o QuickPaste entrega al comando Tauri una lista ordenada de IDs
   opacos de las capturas seleccionadas. El orden de iteración de la selección
   es el orden de exportación; los rangos usan el orden visible de la lista.
2. Un servicio del core carga cada registro en ese orden. Para capturas de
   texto, incluidas las de rich text, usa el campo de texto plano ya asociado
   al registro y conserva saltos de línea. Para una imagen, carga el PNG
   mediante el almacén de assets validado y lo ajusta a la página sin cambiar
   su proporción.
3. `printpdf` produce un único documento en memoria. Si la preferencia está
   activa, cada captura comienza en una página nueva; el contenido de una
   captura larga puede continuar en páginas adicionales. Si está desactivada,
   las capturas siguen el flujo del documento y solo se crea una página nueva
   cuando el contenido ya no cabe.
4. Una vez creado el PDF completo, el adaptador Tauri abre un diálogo nativo
   con extensión `.pdf` y nombre sugerido `ClipVault-capturas.pdf`. Si se
   cancela, no crea ni modifica archivos. Si se confirma, escribe ese único
   PDF en la ruta elegida y devuelve el resultado a la ventana que inició la
   acción.

## Selección e interfaz

- Desktop: el botón PDF se ubica entre el menú de acciones y el botón de
  borrado. Su acción usa el conjunto seleccionado de `HistoryCardRail` y no
  cambia ni limpia la selección.
- QuickPaste: el botón acompaña al indicador de selección. Se habilita con
  una o más capturas; activar el botón no confirma pegado ni oculta la
  ventana.
- En ambas superficies el botón queda deshabilitado cuando no hay capturas
  seleccionadas y durante una exportación en curso.
- La generación mantiene el orden de selección, no el orden temporal de las
  filas. Si una captura fue alternada fuera y luego de nuevo dentro de la
  selección, se conserva su nueva posición de selección.

## Settings

- Nuevo booleano `pdf_page_break_between_captures`, persistido por
  `SettingsService` y expuesto por `Settings` / `SettingsUpdate`.
- Valor predeterminado: `true`, de modo que cada captura seleccionada
  empiece en una página nueva.
- Configuración general mostrará una casilla con descripción clara. Cambiarla
  persiste el ajuste inmediatamente y un error conserva el valor anterior.

## Generación y privacidad

- El core recibe IDs y lee registros/assets mediante los repositorios y
  almacenes locales existentes. La lista de IDs mantiene el orden solicitado;
  un ID repetido se procesa una sola vez y un ID inexistente o un asset de
  imagen inválido aborta la generación antes de abrir el diálogo.
- El texto se agrega al layout como texto escapado, nunca como HTML de
  captura. Los datos de imagen se pasan al renderizador como bytes validados,
  no como rutas o contenido inyectado en el HTML. El rich text no se modifica
  en SQLite: se exporta el texto plano que la captura ya conserva.
- No se registran contenidos, bytes, rutas ni nombres de archivos en logs.
  ClipVault no conserva una copia del PDF después de exportarlo.
- Se exportan los registros y assets ya guardados en la base local. Las
  capturas importadas previamente al historial local son elegibles; la
  superficie de historial remoto de equipos no expone la acción.

## Dependencias

- `printpdf = 0.12.8` en `clipvault-core`, con el feature HTML/XHTML y soporte
  PNG necesario para layout simple, ajuste de texto, imágenes y páginas.
  La versión requiere Rust 1.88, por debajo del MSRV del workspace (1.90).
  El layout HTML del crate aún evoluciona; el servicio limitará la entrada a
  texto escapado, PNG y propiedades sencillas y verificará contenido, orden y
  paginación con pruebas de PDF. Se incrustará DejaVu Sans y su aviso de
  licencia en el core para que el resultado no dependa de una fuente instalada
  por el usuario.
- `tauri-plugin-dialog = 2.8.3` en `clipvault-app` para diálogo de guardado
  nativo. La integración se mantiene en el adaptador del shell y no añade
  acceso de red ni almacenamiento remoto.

## Errores y cancelación

- Con selección vacía, la UI no permite iniciar la acción y el core rechaza
  igualmente una solicitud vacía.
- Cancelar el diálogo es un resultado normal; no muestra un error ni escribe
  el archivo.
- Fallos al resolver capturas/assets, generar el PDF o guardar el destino se
  comunican con claves traducidas en los cinco idiomas.
- Una exportación activa bloquea un segundo disparo desde la misma ventana.

## Verificación

- Pruebas del core para una captura y varias, un solo documento generado,
  orden de texto, salto de línea, imágenes PNG, las dos políticas de salto de
  página, IDs duplicados y errores de asset/ID.
- Pruebas de settings para el valor predeterminado y el round-trip del nuevo
  booleano.
- Checks y build de frontend/Tauri, paridad de traducciones, validación
  estricta de OpenSpec y `git diff --check`.
- Revisar las regresiones protegidas del drag and drop de cards y las
  selecciones de Desktop/QuickPaste. No se requiere una prueba manual de
  guardado del diálogo si el entorno de test no ofrece una sesión gráfica;
  registrar esa limitación.
