# Proposal: collection-switch-resets-search-and-filters

## Why

Al cambiar la colección desde la barra lateral se reinician algunos filtros,
pero el texto de búsqueda permanece. La colección nueva puede quedar mostrando
un resultado vacío o limitado por criterios que pertenecían al ámbito anterior.

## What Changes

- Al seleccionar otra colección desde la barra lateral, vaciar el texto de
  búsqueda y cancelar cualquier búsqueda pendiente.
- Restablecer a `Todas` los filtros de aplicación, tag y colección del toolbar.
- Cargar las opciones y las capturas del nuevo ámbito sin reutilizar resultados
  de la búsqueda anterior.
- Mantener seleccionada la colección destino como ámbito activo.

## Capabilities

### Modified Capabilities

- `desktop-filtering`: reiniciar la búsqueda y los filtros del toolbar cuando
  cambia la colección activa desde la barra lateral.

## Impact

- Afecta al estado de búsqueda y filtros en `App.svelte` y a la navegación de
  colecciones del sidebar.
- Reutiliza la carga y los controles existentes; no requiere cambios de base de
  datos, nuevas cadenas de interfaz ni dependencias.
