# Diseño: interfaz multilenguaje

## Catálogos JSON

Mantener un catálogo JSON completo por idioma (en, es, pt, de, fr), con claves
namespaced idénticas. Esta forma mantiene cada idioma legible y permite revisar
traducciones sin repetir el texto de los otros cuatro idiomas en cada entrada.
Los catálogos son la fuente única de textos de interfaz; no se mantienen copias
traducibles en componentes ni en código nativo.

Las claves cubren los textos propios de ClipVault en la ventana principal,
QuickVault, modales, menús, bandeja del sistema, tooltips, etiquetas
accesibles, estados vacíos, carga y errores. El contenido del usuario, nombres
de aplicaciones externas y etiquetas suministradas por el sistema operativo
quedan fuera de los catálogos.

Un check local valida que todos los catálogos tengan las mismas claves y que
ninguna traducción requerida esté vacía. El catálogo inglés sirve como
fallback defensivo ante una clave ausente en ejecución; no sustituye la
obligación de mantener completa la paridad.

## Traducción en ejecución

Un módulo compartido de localización del frontend resuelve claves, parámetros
y formas plurales desde el idioma activo. Para fechas, números y reglas
plurales se usan las API Intl del runtime, evitando cadenas concatenadas y
dependencias de localización adicionales. Las traducciones con valores
dinámicos declaran placeholders con nombres estables.

Las superficies nativas que muestran texto de ClipVault consumen el mismo
catálogo o una representación generada desde él. La bandeja carga los mismos
archivos JSON incluidos en el binario con `include_str!`; no mantiene un segundo
conjunto manual de traducciones. El core de plataforma entrega identificadores
estables tipados para la guía de permisos y la interfaz resuelve las claves de
catálogo. Así la lógica de plataforma no contiene títulos, resúmenes ni pasos
traducibles.

## Preferencia y propagación

Guardar el locale seleccionado como preferencia de aplicación en la
configuración local existente. Los valores admitidos son en, es, pt, de y fr;
las instalaciones existentes, valores ausentes o valores no válidos usan en.
No se detecta ni se hereda automáticamente el idioma del sistema.

El selector de Configuración general guarda el cambio de inmediato. El estado
de idioma activo se propaga a las ventanas abiertas (incluida QuickVault) y a
los menús nativos reconstruibles, sin reiniciar la aplicación. Ambos puntos de
entrada esperan a cargar la preferencia persistida antes de montar su interfaz;
el comando de configuración emite un evento de cambio después de guardar y
reconstruye la bandeja con el nuevo locale. Si no se puede guardar la
preferencia, la interfaz conserva el idioma anterior y muestra el error
localizado en ese idioma.

## Restricciones

- La localización funciona offline y no agrega servicios de red.
- La configuración existente es key-value, por lo que la preferencia no
  requiere una migración de esquema SQLite.
- La lógica de catálogo y locale permanece separada de la lógica de dominio;
  Tauri solo coordina persistencia y eventos entre superficies.
- Los textos no deben incorporar contenido sensible en logs al reportar
  errores de traducción o persistencia.
