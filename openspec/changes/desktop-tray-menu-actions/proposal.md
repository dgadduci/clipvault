# Propuesta: completar las acciones del menú de bandeja

## Why

El menú nativo de la bandeja muestra acciones que no están conectadas:
`Favorites`, `Clear History` y `Settings`. `Open ClipVault` tampoco garantiza
que una ventana principal ya abierta vuelva al frente, y el menú no ofrece el
modal Acerca de que ya existe en la ventana principal. Esto deja sin efecto
varias acciones habituales del acceso de bandeja en macOS y Linux.

## What Changes

- Hacer que `Open ClipVault` reutilice la ventana principal existente,
  restaurándola si está minimizada, mostrándola y dándole foco. Una activación
  repetida no crea ventanas principales adicionales.
- Quitar `Favorites` solamente del menú nativo de bandeja; la función de
  favoritos dentro de ClipVault permanece disponible.
- Conectar `Clear History` con la confirmación existente y con la operación
  que borra capturas no favoritas sin membresías en colecciones de usuario.
  La membresía obligatoria en `Historial` no cuenta como colección de usuario.
- Hacer que `Settings` muestre la ventana principal y abra el modal existente
  de Configuración general.
- Agregar `About`, que muestra la ventana principal y abre el modal Acerca de
  existente, compartido con el menú de la ventana.
- Localizar la nueva etiqueta `About` en todos los idiomas disponibles.

## Capabilities

### Modificadas

- `desktop-platform-integration`: contrato y comportamiento de las acciones
  del menú de bandeja.
- `clipboard-history-cards`: puntos de acceso al mismo modal Acerca de.

## Fuera de alcance

- Quitar favoritos de la aplicación o cambiar su comportamiento fuera del
  menú de bandeja.
- Cambiar las acciones de búsqueda rápida, pausar/reanudar capturas o salir.
- Cambiar el predicado, la confirmación o la operación de borrado del
  historial existente.
- Crear modales o ventanas nuevos para Configuración o Acerca de.
- Cambiar la persistencia, la sincronización entre equipos o los datos de
  colecciones.

## Resultado esperado

Las acciones del menú de bandeja llevan al usuario a la ventana y al flujo
existentes que corresponden. `Clear History` elimina únicamente las capturas
no favoritas y sin colección de usuario, tras confirmación; `Favorites` deja
de aparecer como acción sin efecto.
