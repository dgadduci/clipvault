# Propuesta: establecer confianza y canales de ayuda para el repositorio

## Problema

El manifiesto Cargo declaraba `MIT OR Apache-2.0`, pero el repositorio no
contenía un archivo de licencia en la raíz. El mantenedor eligió GPL versión 3
para el proyecto. Tampoco hay indicaciones visibles para
reportar problemas, contribuciones o vulnerabilidades. Una persona nueva no
puede confirmar fácilmente los términos del código ni dónde pedir ayuda.

## Qué cambia

- Cambiar la metadata de todos los componentes propios a `GPL-3.0-only` y
  añadir el texto estándar completo de GNU GPL versión 3 como `LICENSE`.
- Añadir una guía breve para contribuir con los límites y pasos de desarrollo
  reales del repositorio.
- Añadir una guía de soporte y plantillas GitHub para errores y propuestas,
  aceptando reportes en inglés o español.
- Añadir instrucciones de seguridad que no expongan públicamente datos
  sensibles ni prometan un canal privado que no esté habilitado.

## Fuera de alcance

- Cambiar el dueño, la visibilidad o la configuración remota del repositorio.
- Adoptar un código de conducta o proceso de gobernanza sin una decisión del
  mantenedor.
- Prometer tiempos de respuesta o soporte comercial.
- Recopilar información del portapapeles, telemetría o datos de diagnóstico
  sensibles.

## Resultado esperado

Los archivos públicos del repositorio explican de forma consistente la
licencia, el soporte, el reporte responsable de problemas y la contribución.
