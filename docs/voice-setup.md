# Transcripción local

La captura usa el micrófono elegido de la Mac o de los auriculares. No presupone que el MR-01 tenga micrófono. La transcripción es una integración opcional: requiere un ejecutable y un modelo que vos ya tengas instalados. La app no descarga modelos ni envía el audio a un servicio de transcripción.

No se incluye un motor de transcripción. Instalá y configurá el que prefieras antes de activar la inserción de texto.

## Contrato del ejecutable

El objeto `TranscriptionCommand` acepta esta forma (ejemplo para adaptar, no ejecutable incluido):

```json
{
  "program": "/ruta/absoluta/a/mi-transcriptor",
  "args": ["--audio", "{audio}"],
  "insert_text": true
}
```

`program` identifica un ejecutable local. Preferí una ruta absoluta, porque una app abierta desde Finder puede tener un `PATH` distinto del de Terminal. Cada elemento de `args` es un argumento separado; al menos uno debe contener `{audio}`, que se reemplaza por la ruta del WAV. No agregues comillas de shell dentro del JSON: los espacios, `$`, punto y coma y otros caracteres se pasan literalmente. La app no interpone un shell.

El ejecutable debe escribir **solo la transcripción final en UTF-8 a stdout**, terminar con código 0 y enviar sus mensajes de progreso a stderr. Puede ser un wrapper que invoque tu motor local, lea su archivo de texto y lo imprima. Revisá las opciones de tu versión del motor: no todos imprimen texto limpio a stdout. Un wrapper es código de confianza que vos configurás; la app no puede impedir que ese programa haga conexiones de red por su cuenta.

El límite es 120 segundos y 64 KiB por cada flujo de salida (stdout y stderr). Se rechazan la salida vacía, los bytes que no sean UTF-8, los errores del proceso y los excesos de tiempo o tamaño. Los diagnósticos del proceso no se copian al log de la aplicación. En macOS y Unix se cancela el grupo de procesos creado para el comando; en otras plataformas se termina el proceso directo.

`insert_text` es una preferencia del flujo de voz que consume el resultado. El runner de transcripción devuelve el texto y no escribe por sí mismo en otra aplicación. La captura, transcripción e inserción deben tener estados visibles; no hay que insertar un mensaje de error como si fuera dictado.

## Validación antes de dictar

1. Probá tu ejecutable manualmente con un audio de prueba y verificá que stdout contiene únicamente lo que se dijo.
2. Configurá el objeto del motor, seleccioná la entrada de audio y asigná la acción de voz a un botón observable del control.
3. Probá la grabación y luego la transcripción con un texto corto. La app necesita permiso de micrófono; la inserción puede requerir Accesibilidad.
4. Para probar la inserción, enfocá un documento de prueba antes de iniciar el dictado. No uses campos de contraseñas ni una terminal como primer destino.

La verificación automatizada del runner utiliza ejecutables de prueba locales: comprueba argumentos literales, texto vacío, UTF-8, errores del proceso, límites de ambos flujos y cancelación por tiempo. No sustituye una prueba de grabación → motor real → aplicación de destino.
