# Diseño: captura de imágenes sucesivas en Ubuntu Wayland

## Contexto

La especificación general ya define la captura de imágenes compatibles, su
persistencia local y la deduplicación por contenido persistido. No describe
que la observación del portapapeles deba continuar después de guardar una
imagen. La regresión reportada aparece en Ubuntu Wayland tras una captura
correcta.

La causa confirmada está en `FallbackRevisionState::observe_payload` del
adaptador `arboard`. El fallback compara únicamente el texto y produce una
revisión conocida. Al pasar de texto a una imagen, la revisión incrementa; al
pasar de una imagen a otra, ambas observaciones tienen `text_leg = None`, por
lo que la revisión no cambia. `CaptureWatcher` confía primero en toda revisión
conocida y devuelve `Unchanged` antes de comparar el fingerprint de imagen.

El fallo no requiere una API nueva de Wayland: el watcher ya compara
fingerprints de payload cuando recibe `ClipboardRevision::UNKNOWN`. La
corrección alinea la señal del adapter con el contrato existente para hosts
sin eventos confiables.

## Objetivos

- Capturar varias imágenes compatibles y distintas durante una misma sesión
  Ubuntu Wayland.
- Mantener la política actual para capturas idénticas.
- Recuperarse de un fallo transitorio de lectura y procesar una captura
  posterior.
- Probar la coordinación sin depender de un portapapeles real en los tests del
  core.

## No objetivos

- Cambiar formatos admitidos, normalización, hashing, persistencia o esquema.
- Cambiar el orden de prioridad entre texto e imagen.
- Introducir dependencias, servicios de red o mecanismos específicos de
  detección de aplicaciones.
- Generalizar la garantía a otros compositores o distribuciones sin pruebas.

## Decisiones

### La sesión de captura permanece activa

Cuando no hay una fuente confiable de revisiones, el adapter devuelve
`ClipboardRevision::UNKNOWN` para cada observación. El watcher compara entonces
el fingerprint del payload completo, incluida la geometría y los pixeles de
una imagen. No se usa un contador sintético derivado solo de texto como si
fuera una revisión del portapapeles. En X11 con XFixes disponible se conserva
la revisión autoritativa existente.

### Se conserva la deduplicación actual

El contenido persistido distinto debe producir una entrada de imagen distinta
y conservar intactas las entradas anteriores. La repetición del mismo
contenido sigue usando la política actual para duplicados; esta corrección no
debe crear filas duplicadas.

### Un error de lectura es recuperable

Un error transitorio al leer o convertir una imagen no debe detener la
observación. Una imagen compatible publicada después debe poder capturarse.
Los errores y diagnósticos no incluirán pixeles, bytes, rutas ni el contenido
del portapapeles.

## Verificación prevista

- Prueba aislada del pipeline con dos o más imágenes distintas sucesivas y
  comprobación de entradas y assets independientes.
- Prueba de una imagen idéntica para proteger la deduplicación vigente.
- Prueba de fallo transitorio seguido de una captura válida.
- Verificación manual en Ubuntu Wayland con el flujo de captura que reprodujo
  el problema, manteniendo ClipVault abierto entre capturas.

La verificación manual registrará solo metadatos de entorno necesarios para
reproducir el caso; no guardará ni expondrá el contenido de las capturas.
