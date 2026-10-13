# Configuración de ACA Studio

La UI y el motor comparten `studio::Config`, versionado con `version: 1`. [config.example.json](config.example.json) es la salida real del modelo por defecto (`cargo run --example configuration`), con fuentes MR-01 integradas y funciones originales por defecto. Guarda las acciones por botón en `normal` y `air_mouse`.

Los cambios se guardan en `~/Library/Application Support/ReadyForAirLab/config.json`; se conserva ese directorio para continuar usando los datos del laboratorio. Guardar crea primero un archivo temporal y preserva `config.previous.json`. Un JSON inválido no se reemplaza silenciosamente: corregilo y recargá. Versiones futuras, botones duplicados, fuentes ambiguas y macros demasiado profundas se rechazan.

El MR-01 incluye un [mapa integrado de botones](mr01-button-map.md). Las configuraciones anteriores con fuentes vacías se completan automáticamente, respetando las señales que el usuario ya haya reasignado. Elegí el botón en 3D y su función: se guarda y usa en la siguiente pulsación, sin identificación ni reconexión. La identificación manual queda en opciones avanzadas para variantes del firmware. El botón físico de modo mouse no emite una pulsación independiente conocida y no puede reasignarse; sus paquetes de movimiento seleccionan el perfil Air mouse. La asociación de Atrás con F1 todavía requiere una comprobación física aislada.

## Acciones

Cada `normal` o `air_mouse` acepta uno de estos objetos:

```json
{"type":"default"}
{"type":"disabled"}
{"type":"shortcut","key":"c","modifiers":["command"]}
{"type":"system","command":"mission_control"}
{"type":"scroll","x":0,"y":-50}
{"type":"apple_script","script":"display notification \"Hola\" with title \"Ready For Air\""}
{"type":"shell","command":"/usr/bin/say Hola"}
{"type":"macro","steps":[{"type":"shortcut","key":"c","modifiers":["command"]},{"type":"system","command":"mission_control"}]}
{"type":"voice_toggle"}
```

Son ejemplos independientes, no un documento JSON único. Los atajos admiten `command`, `control`, `option`, `shift`; consultá `actions.rs` para las teclas admitidas. Los scripts son código elegido por el usuario y se ejecutan con sus permisos. Editarlos, seleccionarlos o guardarlos no los ejecuta.

Las acciones de sistema disponibles incluyen `mission_control`, `app_expose`, `show_desktop`, `launchpad`, `volume_up`, `volume_down`, `mute`, `zoom_in`, `zoom_out`, `zoom_toggle`, `display_sleep`, `sleep` y `lock_screen`. Zoom usa los atajos de accesibilidad configurados en macOS; no activa esa preferencia automáticamente. `launchpad` conserva su nombre en JSON, pero abre Apps.app en Tahoe y Launchpad.app en versiones anteriores. Mostrar escritorio y Ventanas de la aplicación utilizan el lanzador de Mission Control del sistema; los argumentos 1 y 2 fueron inspeccionados en macOS 26.6.2 y requieren revalidación en nuevas versiones.

La selección del perfil en el editor solo cambia qué configuración editás. La actividad del puntero se infiere de paquetes HID recientes. Las flechas marcan el retorno al funcionamiento normal; al cesar los paquetes durante `mouse_idle_ms` (1500 por defecto) se vuelve al perfil Normal como política de la app. No existe un comando comprobado para reactivar el sensor desde la Mac.

## Usar las asignaciones

Al abrir Studio, el control emparejado se detecta y las acciones se habilitan automáticamente cuando los permisos y la captura están disponibles. Si necesita autenticación de administrador, Studio presenta un único diálogo de macOS; no abre Terminal. Cancelar no vuelve a abrir el diálogo automáticamente.

1. Seleccioná un botón del modelo 3D y el perfil Normal o Air mouse.
2. Elegí su función. Se guarda y aplica automáticamente; no hay activación por botón.
3. Usá el botón físico. No hay un paso de aprendizaje en el flujo habitual. **Volver a identificar** es solo una opción avanzada si una variante del control usa otra señal.

**Pausar control / Reanudar control**, en la cabecera, afecta a todas las funciones. Para scripts/macros, usá **Guardar acción JSON** en Opciones avanzadas. Guardar nunca ejecuta la acción seleccionada y no requiere que el lector esté activo. Si falla la escritura o hay cambios externos en el archivo, no se reemplaza la configuración anterior.

La captura sigue siendo una sesión de hasta 30 minutos y termina al cerrar Studio. Si caduca o se cancela la autorización, **Reanudar lector** permite renovarla; no hay una cadena de solicitudes automáticas. Esto aún no instala un servicio permanente. El lector requiere Monitoreo de entrada; Studio, Monitoreo de entrada y Accesibilidad. Dejá Modify events desactivado para el MR-01 en Karabiner para evitar competir por el dispositivo.

Ver [motor nativo y límites](native-engine.md).

Para voz, elegí una entrada de la Mac o auriculares y configurá `transcription` según [voice-setup.md](voice-setup.md). Una pulsación inicia la grabación y otra la detiene. El texto se guarda; solo se intenta insertarlo si sigue activa la misma aplicación de destino y la asignación continúa habilitada.

## Borradores y ejecución acotada

Cada botón/perfil conserva su borrador JSON al navegar. Un cambio independiente (por ejemplo, micrófono) no reemplaza un JSON editado. Si cambió el modelo base, se rechaza guardar encima sin revisión; **Descartar borrador y recargar** es explícito. La selección de audio persiste antes de cambiar el modelo en memoria.

Una pulsación espera como máximo 500 ms antes de comenzar a ejecutarse. Una macro que ya comenzó conserva su presupuesto de 15 segundos. Pausar, editar configuración o cerrar Studio cancela el trabajo anterior. Al terminar un shell se termina también su grupo: procesos desacoplados deliberadamente no forman parte del contrato de acciones. La transcripción comparte esta limpieza y se cancela con el mismo ciclo de vida.
