# Diseño: confianza y soporte del repositorio

## Licencia

La decisión del mantenedor es usar GPL versión 3 únicamente, sin la cláusula
“o cualquier versión posterior”. Declarar `GPL-3.0-only` en
`[workspace.package].license` y mantener los paquetes miembros heredando esa
metadata. Alinear también la cabecera SPDX del script KWin incluido en el
producto. Su `metadata.json` debe usar la palabra clave `GPLv3` que reconoce
KDE para la versión 3 sin “or later”; la cabecera fuente conserva el SPDX
`GPL-3.0-only`. Añadir en la raíz un `LICENSE` con el texto estándar
íntegro de GNU GPL v3, sin reescribirlo ni añadir términos propios. Validar la
expresión mediante Cargo y conservar los enlaces de la licencia al repositorio
oficial.

## Ayuda y contribuciones

Agregar documentación breve en inglés y español. `CONTRIBUTING.md` explicará
cómo preparar el toolchain ya usado por el proyecto, ejecutar verificaciones
relevantes y proponer cambios acotados; enlazará `AGENTS.md` y
`docs/development.md` para detalles internos en lugar de duplicarlos.

`SUPPORT.md` orientará a las personas a las guías de instalación y a los
GitHub Issues para problemas reproducibles. Los formularios de Issues
solicitarán versión, sistema, escritorio y sesión cuando corresponda, pero
advertirán que no se debe adjuntar historial, capturas con contenido privado,
tokens ni archivos de configuración secretos.

## Reportes de seguridad

`SECURITY.md` explicará que los reportes sensibles no deben publicarse como
issues. La consulta de solo lectura del 2026-10-07 confirmó que GitHub Private
Vulnerability Reporting está deshabilitado y no hay un contacto privado
publicado. El documento debe decirlo claramente y no solicitar detalles hasta
que el mantenedor habilite una ruta privada o publique un contacto. No cambiar
settings remotos ni inventar una dirección.

## Idiomas

Los documentos de cara a personas usuarias y plantillas ofrecerán instrucciones
equivalentes en inglés y español, ya sea en archivos separados enlazados o en
una misma página claramente segmentada. Los identificadores de campos de
GitHub pueden conservarse en el formato que acepte la plataforma.

## Límites

No se agregan dependencias de aplicación, servicios externos, insignias de
certificación ni compromisos de soporte. Los documentos describen los checks
existentes y no ordenan ejecutar la suite completa para todo cambio.
