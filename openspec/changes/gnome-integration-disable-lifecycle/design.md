# Diseño: desactivación GNOME sin bloquear ClipVault

## Recorrido de desactivación

1. ClipVault invoca la interfaz pública conocida `gnome-extensions disable`
   con el UUID constante de su propia extensión. El adaptador usa una ruta de
   ejecutable fija, sin shell ni argumentos suministrados por la interfaz.
   Espera desde un worker y aplica un límite de tiempo; si vence, termina el
   proceso hijo y devuelve un error recuperable.
2. GNOME Shell desactiva la extensión y esta cierra su conexión local.
3. El listener termina de forma cooperativa. El hilo comprueba la solicitud
   de apagado mientras procesa una conexión persistente; el comando Tauri no
   ejecuta esperas bloqueantes en el hilo de interfaz ni espera un `join` que
   pueda quedar bloqueado por una lectura de socket sin límite.
4. ClipVault retira sus archivos locales, cambia el consentimiento a
   `disabled`, actualiza el estado técnico y vuelve a consultar el estado para
   la interfaz.
5. La interfaz deja de mostrar progreso y presenta **Deshabilitada**, con la
   acción para habilitarla de nuevo. Ese estado debe mantenerse al reiniciar
   ClipVault.

## Errores y recuperación

- Si el ejecutable de GNOME no está disponible, falla o supera el tiempo
  límite de desactivación, no se retiran los archivos de la extensión ni se
  confirma `disabled`. ClipVault conserva la instalación y el consentimiento
  anteriores, detiene el indicador de progreso, muestra un error traducido y
  permite volver a intentarlo.
- Si la extensión ya está desconectada, el cierre del listener debe seguir
  siendo idempotente y terminar con rapidez.
- Una solicitud de apagado debe poder interrumpir la lectura de un peer que
  permanece conectado sin enviar datos. Se puede usar una lectura con timeout
  breve y consultar el indicador de vida entre lecturas; cualquier ajuste del
  transporte debe permanecer dentro de `clipvault-platform`.
- Si una fase local posterior a la desactivación del Shell falla, ClipVault
  mantiene el consentimiento desactivado y devuelve un estado recuperable; no
  debe reactivar la extensión de forma implícita.

## Fronteras

- El controller de GNOME existente proporciona una operación inyectable para
  habilitar. Se agrega su pareja de desactivación para que las pruebas puedan
  simular éxito, ejecutable ausente y error del proceso.
- El adaptador de plataforma usa únicamente el ejecutable fijo y el UUID de
  ClipVault. No ejecuta comandos recibidos desde el frontend ni necesita
  privilegios.
- La extensión comunica solo metadatos permitidos por el protocolo existente.
  La desactivación no lee ni escribe contenido del portapapeles.
- Todos los mensajes nuevos o modificados se localizan en `en`, `es`, `pt`,
  `de` y `fr`.
