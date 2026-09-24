# Transferencia local de texto entre pares

> Este cambio es el contrato arquitectónico paraguas. No se entrega mediante
> MiniMax de forma directa. La implementación se realiza, en este orden, con
> `local-peer-identity-foundation`, `local-peer-discovery`,
> `local-peer-mutual-pairing`, `peer-text-history-browser` y
> `peer-text-import`.

## Objetivo

Permitir que cualquier cantidad de instalaciones de ClipVault que estén en la
misma red local se descubran, se vinculen de a pares por aprobación mutua y
transfieran manualmente capturas de texto. Cada vínculo es independiente; no
es una función de sincronización automática ni conecta con Internet.

## Artefactos

- `proposal.md`: alcance de producto aprobado.
- `design.md`: arquitectura, seguridad, persistencia y decisiones técnicas.
- `specs/local-peer-sharing/spec.md`: identidad, opt-in, descubrimiento,
  estados y vínculo mutuo.
- `specs/peer-text-import/spec.md`: historial remoto de texto e importación
  explícita.
- `tasks.md`: orden de implementación y evidencia requerida.

## Secuencia de implementación

1. Identidad criptográfica y perfil local, sin abrir red.
2. Descubrimiento persistente mDNS, sin acceso remoto.
3. Vínculo recíproco y transporte TLS autenticado, sin historial remoto.
4. Navegación paginada de previews de texto, sin descargar texto completo.
5. Importación explícita, colección vinculada y procedencia local.

## Decisiones aprobadas

- Compartición desactivada por defecto y sólo en la red local.
- Descubrimiento continuo DNS-SD/mDNS, sin escaneo de puertos ni ingreso de IP
  manual en esta primera entrega.
- Nombre visible editable, con identidad criptográfica estable independiente.
- Código corto idéntico y aceptación explícita en ambos equipos, una única
  vez; después, autenticación silenciosa por claves fijadas.
- Acceso recíproco después del vínculo, exclusivamente a capturas de texto
  transferibles, paginadas y con preview directo acotado.
- Importación explícita a una colección local vinculada al identificador del
  par; el título remoto se conserva cuando es válido.
- Las importaciones ya realizadas sobreviven a desconectar o bloquear un par.
