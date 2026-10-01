# Tareas

## 1. Catálogos y validación

- [x] 1.1 Inventariar los textos propios visibles en la ventana principal,
  QuickVault, componentes, menús, bandeja, errores, estados y etiquetas
  accesibles.
- [x] 1.2 Crear catálogos JSON completos para en, es, pt, de y fr, con claves
  namespaced, placeholders consistentes y traducciones revisadas.
- [x] 1.3 Agregar una validación local de paridad de claves, traducciones no
  vacías y placeholders requeridos.

## 2. Idioma activo y configuración

- [x] 2.1 Añadir a la configuración local el locale admitido, con en como
  valor por defecto y fallback seguro para valores ausentes o inválidos.
- [x] 2.2 Implementar el módulo reactivo de traducción con fallback al inglés,
  placeholders y formato locale-aware para plurales, fechas y números.
- [x] 2.3 Agregar el selector de idioma a Configuración general y guardar cada
  selección inmediatamente.
- [x] 2.4 Propagar cambios guardados a la ventana principal, QuickVault y
  superficies nativas localizables durante la sesión.
- [x] 2.5 Mantener el locale anterior y mostrar un error localizado si falla
  la persistencia.

## 3. Migración de textos y verificación

- [x] 3.1 Reemplazar los textos fijos por claves en todos los componentes y
  superficies incluidos en el alcance.
- [x] 3.2.1 Verificar paridad de catálogos, persistencia de locale, traducción,
  placeholders, plurales y fallback seguro de configuración.
- [x] 3.2.2 Verificar en ejecución Tauri el cambio en caliente, la propagación
  entre la ventana principal y QuickVault, y la reversión si falla el guardado.
  Verificación manual confirmada por el usuario en todos los equipos.
- [x] 3.3 Ejecutar los checks/builds relevantes de frontend y Tauri, validar
  OpenSpec y revisar el diff.
