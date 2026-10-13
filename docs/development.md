# Desarrollo y empaquetado

Requisitos: Mac, Xcode/Command Line Tools y Rust con Cargo en PATH. El entorno de desarrollo usa Rust 1.99, Apple Silicon y las versiones de Cargo.lock. No hay dependencias sobre rutas personales de otro checkout. `READY_AIR_RUST_TOOLCHAIN` permite elegir un directorio que contenga `bin/cargo`; una selección local también puede guardarse en `.tools/rust-toolchain-path`, ignorada por Git.

```sh
git clone https://github.com/facundoPri/air-controller-adapter.git
cd air-controller-adapter
./script/cargo.sh test --locked --all-targets
./script/build_and_run.sh --build-only
```

El empaquetador genera `dist/ACA Studio.app` y `dist/ACA Capture.app`. Sin `--build-only` abre Studio. `READY_AIR_PROFILE=release` habilita compilación optimizada y `READY_AIR_DIST` cambia la carpeta de salida. Los nombres internos del crate y ejecutables conservan la identidad anterior para mantener continuidad.

```sh
READY_AIR_PROFILE=release READY_AIR_DIST="$PWD/dist/release" ./script/build_and_run.sh --build-only
"dist/release/ACA Studio.app/Contents/MacOS/ReadyForAirStudio" --verify-capture
```

El orden importa: se compila y firma Capture primero; su hash se integra al compilar Studio. Después se firma Studio. Compilar Studio directamente con Cargo sirve para desarrollar, pero no genera un par autorizado para iniciar el lector. `--verify-capture` comprueba el par sin abrir la UI, iniciar lectura ni solicitar privilegios.

La firma por defecto es ad hoc. `READY_AIR_SIGN_IDENTITY` permite elegir una identidad instalada; eso por sí solo no agrega notarización ni certifica el flujo de distribución.

Los tests comprueban en aislamiento informes HID, colas, cancelación, persistencia, borradores, permisos observados y arranque del lector. No conceden permisos, no prueban efectos reales sobre el escritorio ni reemplazan una instalación de prueba en otra Mac. Los tests opcionales de render y preferencias Zoom se omiten de forma predeterminada.

La descarga de preview incluye dos bundles, README y avisos de dependencias. Excluye datos locales, logs de hardware, notas de coordinación y material de referencia de terceros.

Para preparar un ZIP a partir del par optimizado ya compilado, con el commit de origen registrado, firmas verificadas antes y después de extraer y checksum SHA-256:

```sh
./script/package_release.sh v0.1.0-preview.1
```

El resultado queda en `dist/releases/v0.1.0-preview.1/`. `THIRD_PARTY_NOTICES.md` corresponde a Cargo.lock y al árbol de dependencias macOS arm64 (incluye herramientas de compilación). Actualizá el inventario y los textos originales de licencia al cambiar dependencias.
