# Metadata de origen GNOME

## ADDED Requirements

### Requirement: Capturas de aplicaciones nativas GNOME Wayland

Las capturas provenientes de aplicaciones nativas GNOME Wayland SHALL conservar el identificador de aplicación cuando la integración opcional esté conectada y SHALL reutilizar el flujo Linux existente para nombre e icono.

#### Scenario: Card enriquecida

- GIVEN una captura con `source_app` obtenido desde GNOME Shell
- WHEN se cargan Desktop y Quick Paste
- THEN pueden mostrar `source_app_name` e `source_app_icon_ref` resueltos por el provider Linux
- AND no se modifican contenido, timestamp, título, tags, colecciones ni favorito

#### Scenario: Integración ausente

- GIVEN una captura tomada sin integración GNOME conectada
- WHEN se renderiza la captura
- THEN conserva el estado accesible de aplicación desconocida
- AND no muestra un nombre inventado desde el título de ventana
