# Diseño: serialización textual versionada de la semilla local

## Decisión

`KeychainPeerIdentityStore` seguirá usando la entrada existente de `keyring`
y `get_secret` / `set_secret`, pero almacenará bytes ASCII en lugar de los 32
bytes aleatorios crudos. El formato exacto será
`clipvault-peer-seed-v1:<hex minúsculo de 64 caracteres>`. Se codifica en el
adaptador de plataforma: core, UI, SQLite, pairing y discovery continúan
recibiendo sólo los tipos de identidad actuales.

La serialización es determinista, reversible y no requiere una dependencia
criptográfica adicional. El prefijo permite distinguir futuras versiones y
evita interpretar texto arbitrario como semilla. El tag no añade protección
criptográfica: la confidencialidad sigue dependiendo del almacén seguro del
sistema.

## Carga, creación y migración

Con el mutex actual protegiendo toda la secuencia:

1. Si el almacén no tiene entrada, generar 32 bytes aleatorios, codificarlos,
   persistirlos y derivar la identidad de esos bytes.
2. Si la entrada contiene el formato v1, validar prefijo, longitud y todos los
   caracteres hexadecimales antes de reconstruir la semilla.
3. Si contiene exactamente 32 bytes legacy, conservarlos y reemplazar el
   payload por su codificación v1. Si la migración falla, devolver el error
   seguro existente en lugar de anunciar una identidad que no sobrevivirá al
   reinicio.
4. Si la entrada está vacía, regenerarla: cero bytes nunca fueron una semilla
   válida y el caso observado deja al usuario sin identidad utilizable.
5. Si contiene otros bytes o una codificación desconocida, fallar de forma
   tipada sin sobrescribirla. No registrar el payload, su representación ni
   la causa nativa.

Las rutas `load_or_create` y `load_or_create_material` deben usar las mismas
funciones de lectura/escritura para que el perfil y el material de TLS no
diverjan.

## Compatibilidad y alternativas

El cambio revisa la decisión del diseño archivado de persistir sólo bytes
crudos. Mantener ese formato sólo para Linux añadiría dos representaciones
según plataforma y no evitaría que otro proveedor Secret Service tenga el
mismo límite con datos binarios. Hex ASCII funciona con el contrato común de
`keyring` y mantiene el mismo backend seguro en macOS y Linux. No se cambia la
dependencia ni se añade un archivo fallback.

Las semillas raw de 32 bytes ya existentes se migran preservando identidad.
Las entradas vacías se regeneran porque no pueden representar una identidad
válida. Payloads no vacíos desconocidos siguen fallando para impedir una
rotación silenciosa.

## Pruebas y verificación

- Cubrir round-trip del formato v1, semilla legacy de 32 bytes, migración sin
  cambio de `peer_id`, entrada vacía, prefijo incorrecto, hex inválido y
  longitud incorrecta.
- Probar las rutas de perfil y material TLS con una misma semilla/fake de
  almacenamiento, sin tocar el Secret Service real en tests automatizados.
- Verificar que errores y logs nunca incluyan semilla ni texto codificado.
- En Arch KDE Plasma Wayland, reiniciar con el item vacío presente, confirmar
  `peer_id` y fingerprint visibles y estables, y comprobar sólo la longitud
  del valor guardado; nunca mostrarlo.
- Validar OpenSpec, formato Rust, tests relevantes y diff. La prueba manual
  KWallet requiere el equipo del usuario.
