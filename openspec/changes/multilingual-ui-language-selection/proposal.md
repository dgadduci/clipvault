# Propuesta: interfaz multilenguaje

## Por qué

La interfaz de ClipVault mezcla textos fijos en distintos componentes y
superficies. Esto dificulta ofrecer una experiencia coherente en otros idiomas
y hace que cada cambio de texto tenga que resolverse de forma aislada.

## Qué cambia

- Centralizar los textos visibles propios de la aplicación en catálogos JSON
  para inglés, español, portugués, alemán y francés.
- Reemplazar los textos fijos de la interfaz por claves traducibles.
- Agregar a Configuración general un selector de idioma con inglés como valor
  predeterminado.
- Persistir la selección localmente y aplicar cada cambio durante la sesión,
  también en las ventanas de ClipVault que estén abiertas.
- Mantener traducciones completas y consistentes entre los cinco catálogos.

## Alcance de los textos

Incluye la ventana principal, QuickVault, diálogos, configuración, mensajes de
estado, errores, tooltips, etiquetas accesibles y superficies propias de la
aplicación como el menú de bandeja. No incluye contenido escrito por el
usuario, nombres entregados por otras aplicaciones ni textos que pertenezcan
al sistema operativo.

## Fuera de alcance

- Detección automática del idioma del sistema o del usuario.
- Traducciones descargadas, servicios remotos o telemetría.
- Localizar contenido creado por el usuario.
- Agregar idiomas distintos de los cinco enumerados.

## Impacto esperado

- Frontend Svelte/TypeScript, incluyendo las ventanas principal y QuickVault.
- Persistencia y validación de la preferencia de idioma en configuración.
- Integración Tauri para propagar cambios a ventanas abiertas y actualizar
  superficies nativas localizadas.
- Catálogos JSON completos y una verificación de paridad de claves.

Es un cambio transversal de interfaz y configuración, aunque no altera el
modelo de capturas, colecciones ni el flujo de portapapeles.
