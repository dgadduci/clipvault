# Diseño: fila desplazable para los controles del toolbar

El contenedor de la búsqueda, los filtros, «Texto» y las acciones será un
flex row sin wrap y con desplazamiento horizontal nativo. Los controles
conservarán un ancho mínimo para que el desplazamiento revele controles
completos en lugar de comprimirlos. La altura automática del flex row seguirá
la altura intrínseca de su control más alto, aunque cambie qué acciones están
disponibles.

El desbordamiento horizontal recorta los descendientes que sobresalen del
contenedor, incluidos los menús desplegables. Una utilidad compartida moverá
cada menú abierto al `body`, lo posicionará con coordenadas del viewport a
partir de su botón y actualizará su posición durante resize y scroll. Así los
menús conservan su tamaño, navegación y eventos actuales sin quedar recortados
por el área desplazable.

No se cambian las acciones ni su orden. El scrollview de la fila será
accesible por teclado y tendrá un nombre descriptivo.
