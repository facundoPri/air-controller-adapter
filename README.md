# ACA — Air Controller Adapter

Personalizá tu **Motorola Ready For Air Remote MR-01** desde una app nativa para Mac, escrita en Rust con GPUI. Seleccioná un botón en el modelo 3D, elegí su función y usalo: la asignación se guarda automáticamente.

**Proyecto independiente, sin afiliación ni respaldo de Motorola.** Los nombres de dispositivos se usan para identificar compatibilidad.

## Descargar

[**Descargar la versión de prueba**](https://github.com/facundoPri/air-controller-adapter/releases/tag/v0.1.0-preview.1) · [Todas las versiones](https://github.com/facundoPri/air-controller-adapter/releases)

En **Assets**, descargá `ACA-v0.1.0-preview.1-macos-arm64.zip`. Los archivos “Source code” son el código fuente, no la app.

Esta es una **preview experimental para Mac con Apple Silicon (M1 o posterior)**. No requiere Rust ni Karabiner. Necesita un MR-01 y una cuenta con capacidad de autorizar al lector como administrador. Se probó el desarrollo en macOS 26.6.2; el bundle declara macOS 13 como mínimo, pero las versiones anteriores no tienen validación completa. No hay binarios para Intel, Windows o Linux.

La descarga tiene firma local **ad hoc**, sin Developer ID ni notarización de Apple. La instalación y los permisos en una Mac nueva todavía requieren validación. El lector funciona en sesiones de **hasta 30 minutos**, renovables desde la app; no es aún un servicio permanente.

## Instalar y conectar

1. Descomprimí el ZIP. Mové la carpeta **ACA** completa a Aplicaciones (o a `~/Applications`). Conservá **ACA Studio.app** y **ACA Capture.app** juntas; no mezcles aplicaciones de versiones diferentes.
2. Abrí **ACA Studio**. Capture es el lector auxiliar, no una segunda interfaz que debas abrir manualmente.
3. Si macOS bloquea la app por su firma de desarrollo, revisá **Ajustes del Sistema → Privacidad y seguridad → Abrir igualmente**, siguiendo la [guía oficial de Apple](https://support.apple.com/102445). Esto autoriza esa app; no hace falta desactivar Gatekeeper globalmente. Si tu Mac no ofrece esa opción, guardá el mensaje exacto para reportarlo.
4. Si el control ya está conectado por Bluetooth, Studio lo detecta automáticamente. Para uno nuevo, mantené **Atrás + OK** hasta que parpadee el LED y elegí **Buscar y conectar** en Conexión. Si aparecen varios controles, seleccioná el tuyo. Apagá temporalmente el Bluetooth del teléfono si está tomando la conexión.
5. Abrí **Conexión → Preparar esta Mac** y completá los permisos indicados abajo. Usá los enlaces a Ajustes y Finder para agregar las aplicaciones exactas desde la carpeta instalada.
6. Autorizá el diálogo de administrador que Studio presenta para iniciar Capture. No se abre Terminal. Después comprobá el estado del lector y pulsá un botón para verificar que llega actividad.

### Permisos

| Permiso | Aplicación | Para qué se usa |
| --- | --- | --- |
| Monitoreo de entrada | ACA Studio **y** ACA Capture | Observar y capturar únicamente el control compatible |
| Accesibilidad | ACA Studio | Reenviar cursor/teclas y ejecutar atajos |
| Bluetooth | ACA Studio | Buscar y conectar el control |
| Micrófono, opcional | ACA Studio | Grabar desde la Mac o auriculares |
| Automatización, cuando se solicite | ACA Studio | Acciones AppleScript dirigidas a otra app |

Los permisos se conceden en **Ajustes del Sistema → Privacidad y seguridad**. En Monitoreo de entrada y Accesibilidad, agregá la app con **+** si no aparece. Si macOS pide cerrar y reabrir, aceptalo y abrí Studio nuevamente. Capture informa su propio permiso cuando arranca; Studio distingue entre permiso observado, canal abierto y paquetes recibidos.

Capture usa privilegios de administrador solo para leer el MR-01. Las acciones y scripts se ejecutan como tu usuario. No se instala un controlador, servicio de inicio ni extensión. Si ya usás Karabiner, desactivá **Modify events solo para este control** para que no compitan por él.

## Personalizar botones

1. En **Buttons**, seleccioná el botón en el modelo 3D o en la lista.
2. Elegí el perfil **Normal** o **Air mouse** que querés editar.
3. Elegí una función: **se guarda y aplica automáticamente** a la próxima pulsación física. No hay que identificar ni activar cada botón.

**Pausar control / Reanudar control** afecta a todas las asignaciones. Para scripts o macros, abrí Opciones avanzadas, editá el JSON y elegí **Guardar acción JSON**. Guardar una función no la ejecuta. Si falla el guardado, la app conserva la configuración anterior y muestra el error.

Hay presets para Mission Control, ventanas de la app, escritorio, aplicaciones, volumen, zoom, Page Up/Down, bloqueo, reposo y apagar pantalla. También atajos, AppleScript, shell y macros. **Zoom necesita los atajos de teclado habilitados en Accesibilidad → Zoom**; ACA no cambia esa preferencia por su cuenta.

### Normal y Air mouse

El movimiento recibido selecciona Air mouse. Una flecha usa la asignación del perfil previo y vuelve a Normal; además, el control apaga su sensor al pulsar una flecha. Tras 1,5 segundos sin reportes de mouse, ACA asume Normal. Esta es una inferencia, no una lectura directa del modo del firmware.

Cambiar el perfil en el editor no activa el sensor. El botón físico Air mouse no tiene un evento de pulsación independiente confirmado, por lo que **no se puede reasignar**. La asociación predeterminada de Atrás todavía requiere una comprobación física aislada; las variantes de firmware pueden usar **Volver a identificar** en opciones avanzadas.

### Voz

Podés asignar una pulsación para empezar a grabar y otra para detener. Usa el micrófono elegido de la Mac o auriculares; **no se confirmó audio del MR-01**. Para insertar texto necesitás configurar un transcriptor local: no se incluye un modelo ni se sube audio automáticamente. [Guía de voz](docs/voice-setup.md).

## Si algo no funciona

| Síntoma | Qué revisar |
| --- | --- |
| El control no aparece | Bluetooth de la Mac, pilas, Atrás + OK y que el teléfono no lo esté usando. Volvé a buscar en Conexión. |
| Se ven paquetes pero no se ejecutan acciones | Estado global, Accesibilidad de Studio, lector activo y perfil Normal/Air asignado. Revisá el motivo en Activity. |
| Termina la sesión o cancelaste la contraseña | Usá **Reanudar lector**. El límite es de 30 minutos; no se repite automáticamente la solicitud cancelada. |
| macOS dice “not permitted” aunque el permiso esté activo | Verificá que autorizaste las apps de esta instalación. Una actualización con firma ad hoc puede requerir quitarlas y agregarlas otra vez, y reabrir Studio. |
| Capture falta o no coincide | Volvé a extraer el ZIP completo. Studio verifica el hash de su lector; no admite mezclar versiones. |
| Zoom no responde | Habilitá los atajos de Accesibilidad → Zoom en macOS. |
| Atajos o scripts fallan | Revisá Activity, la aplicación enfocada y los permisos de Automatización que macOS solicite. |

**Activity** permite inspeccionar y exportar los eventos. Para [reportar un problema](https://github.com/facundoPri/air-controller-adapter/issues), incluí versión de ACA/macOS, modelo de Mac, botón, perfil y acción, pasos y mensaje de error. Revisá los logs antes de adjuntarlos: pueden contener tu configuración o datos del dispositivo.

## Configuración, actualización y desinstalación

El JSON y los datos locales se guardan en `~/Library/Application Support/ReadyForAirLab/`. Ese nombre histórico se conserva para no perder configuraciones anteriores. `config.previous.json` conserva la versión anterior del archivo; las grabaciones y transcripciones también permanecen locales.

Para actualizar, cerrá Studio y reemplazá **ambas apps juntas**, conservando esa carpeta de datos. No ejecutes simultáneamente la versión anterior y ACA. Los identificadores internos mantienen los nombres históricos de Studio/Capture para conservar continuidad; la firma ad hoc aún puede obligar a renovar permisos.

Para desinstalar, cerrá Studio, eliminá la carpeta ACA y retirala de los permisos en Ajustes. Si además querés borrar configuraciones, grabaciones y transcripciones, eliminá la carpeta de datos tras guardar lo que necesites. No hay servicio permanente que desinstalar.

- [Formato JSON y ejemplos](docs/configuration.md)
- [Mapa de botones y sus límites](docs/mr01-button-map.md)
- [Arquitectura del motor](docs/native-engine.md)
- [Compilar, probar y empaquetar](docs/development.md)
- [Notas de la preview](docs/releases/v0.1.0-preview.1.md)

El repositorio incluye código fuente; aún no se eligió una licencia de código abierto para el código propio. Las dependencias conservan sus licencias; sus avisos se incluyen en la descarga.
