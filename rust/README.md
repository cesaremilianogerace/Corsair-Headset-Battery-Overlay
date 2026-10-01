# Corsair Battery Level

Muestra en la bandeja del sistema de Windows (systray) el nivel de batería y el estado de un headset inalámbrico Corsair.

Es un único `.exe` de ~150 KB, sin instalador ni dependencias externas. Usa ~1,5 MB de RAM y no consume CPU mientras espera.

## Funcionalidades

- **Ícono con el porcentaje de batería**, dibujado en tiempo de ejecución (ver [Estilos del ícono](#estilos-del-ícono-de-la-bandeja)).
- **Colores según el estado**: rojo si la batería está baja (≤ 15 % o el headset reporta "Low battery") y verde si está cargando.
- **Tema claro y oscuro**: el ícono se adapta al tema de la barra de tareas y se redibuja al cambiarlo.
- **Nítido en cualquier escala** (100 %, 125 %, 150 %…): se dibuja al tamaño exacto que pide Windows.
- **Tooltip** con el modelo y el estado, por ejemplo `Corsair VOID PRO Wireless: Connected (87%)`.
- **Detección automática**: reconoce el dongle cuando se conecta o se desconecta, sin tocar nada.
- **Menú contextual**: estado actual, *Refresh device* y *Exit*.
- **Una sola instancia**: si ya hay una abierta, la segunda se cierra sola.
- **Se recupera si se reinicia el Explorador de Windows**: vuelve a poner el ícono en la bandeja.

### Modelos soportados

| Modelo | Product IDs |
|---|---|
| HS70 Wireless | `0A38` |
| HS70 PRO Wireless | `0A4F` |
| VOID Wireless | `1B27`, `0A2B` |
| VOID PRO Wireless | `0A14`, `0A16`, `0A1A` |
| VOID ELITE Wireless | `0A55`, `0A51` |
| Virtuoso RGB Wireless | `0A3E`, `0A40`, `0A42`, `0A44`, `0A5C`, `0A64` |

Todos usan el Vendor ID de Corsair, `1B1C`. Para agregar un modelo, sumá su PID en `model_name()` de [src/hid.rs](src/hid.rs).

## Lenguaje y dependencias

Está escrito en **Rust** (edición 2021) y usa directamente la API Win32, sin frameworks de interfaz ni runtimes.

| Dependencia | Para qué | ¿Va dentro del `.exe`? |
|---|---|---|
| [`windows-sys`](https://crates.io/crates/windows-sys) 0.59 | Bindings oficiales de Microsoft a la API Win32 (solo declaraciones, sin capas extra) | Sí, pero solo lo que se usa |
| [`miniz_oxide`](https://crates.io/crates/miniz_oxide) 0.8 | Comprime en PNG el ícono del `.exe` durante la compilación | No, solo se usa al compilar |

En ejecución, el programa solo carga DLLs que ya vienen con Windows: `kernel32`, `user32`, `shell32`, `gdi32`, `advapi32`, `hid`, `cfgmgr32` y el Universal CRT (`ucrtbase`). **No necesita el redistribuible de Visual C++.**

## Estructura

```
rust/
├── Cargo.toml          Dependencias y perfil de release (optimizado para tamaño)
├── .cargo/config.toml  Enlazado del runtime de C (ver "Compilación")
├── build.rs            Genera el ícono del .exe y lo incrusta como recurso
└── src/
    ├── main.rs         Ventana oculta, loop de mensajes, estado de la app y menú
    ├── hid.rs          Descubrimiento del headset y lectura/escritura HID
    ├── draw.rs         Dibujo de todos los íconos (Rust puro, sin Win32)
    ├── icon.rs         Convierte los dibujos en íconos de Windows y lee el tema
    └── tray.rs         Ícono de la bandeja (Shell_NotifyIconW)
```

## Compilación

### Requisitos

1. **Rust** instalado con [rustup](https://rustup.rs), con el toolchain `stable-x86_64-pc-windows-msvc` (el que se instala por defecto en Windows).
2. **Visual Studio 2022** (Community o Build Tools) con la carga de trabajo **"Desarrollo para el escritorio con C++"**, que incluye el linker MSVC y el Windows SDK. rustup ofrece instalarlo si falta.
3. **Smart App Control desactivado** en la PC donde se compila. Si está activo, Windows bloquea el compilador de Rust (`rustc.exe` falla con el código `0xC0E90002`, porque sus DLLs no están firmadas). Se desactiva en *Seguridad de Windows → Control de aplicaciones y navegador → Smart App Control*.

### Comandos

```powershell
cd rust
cargo build --release
```

El ejecutable queda en `target\release\corsair_battery_level.exe` y se puede copiar a cualquier lugar.

Los otros `.exe` que aparecen en `target\release\deps\` y `target\release\build\` son intermedios del compilador: no sirven para usar.

| Comando | Para qué |
|---|---|
| `cargo build --release` | Ejecutable final (~150 KB) |
| `cargo build` | Compilación rápida para desarrollo (`target\debug\`, más grande y lento) |
| `cargo run --release` | Compila y ejecuta |
| `cargo test --release` | Genera una vista previa de los íconos (ver [Vista previa](#vista-previa-de-los-íconos)) |
| `cargo clean` | Borra `target\` (varios cientos de MB de intermedios) |

> Si la app está abierta, Windows no deja sobrescribir el `.exe` y el build falla con "Acceso denegado". Cerrala antes (clic en el ícono → **Exit**, o `Stop-Process -Name corsair_battery_level`).

### Qué hace que el ejecutable sea tan chico

- **Perfil de release** en [Cargo.toml](Cargo.toml): `opt-level = "z"` (optimiza para tamaño), `lto = true`, `codegen-units = 1`, `panic = "abort"` (no incluye el código para recuperarse de errores) y `strip = true` (quita símbolos).
- **"Hybrid CRT"** en [.cargo/config.toml](.cargo/config.toml): el runtime de Visual C++ se enlaza estático y el Universal CRT dinámico, porque `ucrtbase.dll` ya viene con Windows 10/11. Así el `.exe` no depende del redistribuible de VC++ y sigue siendo chico.
- **Sin archivos de imagen**: todos los íconos se dibujan por código.

### Ícono del `.exe`

Windows lee el ícono que muestra el Explorador directamente del archivo, sin ejecutarlo, así que ese no se puede dibujar en tiempo de ejecución. Por eso [build.rs](build.rs) lo genera **al compilar**, con el mismo código de [src/draw.rs](src/draw.rs) (función `draw_app_icon`): un headset gris con una batería al 50 %.

- Incluye 8 tamaños (16 a 256 px) comprimidos en PNG. Pesa ~5,6 KB.
- Se incrusta con un archivo `.res` que escribe el propio `build.rs`, sin necesitar `rc.exe`. El `.ico` suelto queda en `target\release\build\corsair_battery_level-*\out\app.ico`.

Si el Explorador sigue mostrando el ícono anterior, es el caché de íconos de Windows: presioná F5 en la carpeta o copiá el `.exe` a otro lado.

## Estilos del ícono de la bandeja

Hay dos estilos implementados. Se elige uno **al compilar**, con una constante.

### `DigitsOverBar` (activo por defecto)

Dos dígitos grandes de 7 segmentos arriba y una línea de carga en el borde inferior.

- Siempre muestra 2 dígitos: `87`, `09`…
- **100 % se muestra `00`** y **0 % se muestra `--`**. El tooltip siempre tiene el porcentaje exacto.
- La línea ocupa todo el ancho. La parte llena es del color del texto de la barra de tareas (roja si la batería está baja, verde si está cargando) y la parte vacía va atenuada.
- Es el más legible a 16 px, el tamaño del ícono con la escala de pantalla al 100 %: los dígitos miden 12 px de alto con trazo de 2 px.

### `NumberOverBattery`

El porcentaje completo arriba (`100`, `87`, `9`) y una batería chica con relleno abajo.

- Muestra el número real, incluido el `100`; el "1" es angosto para que entre.
- A 16 px los dígitos quedan de 9 px con trazo de 1 px, menos legibles. Se ve mejor con escala de 125 % o más (íconos de 20 px o más).

### Cómo cambiar de estilo

En [src/draw.rs](src/draw.rs), cambiá la constante:

```rust
pub const TRAY_STYLE: TrayStyle = TrayStyle::DigitsOverBar;
// o
pub const TRAY_STYLE: TrayStyle = TrayStyle::NumberOverBattery;
```

y recompilá con `cargo build --release`.

### Colores

Están en las constantes al principio de [src/draw.rs](src/draw.rs):

| Constante | Uso | Valor |
|---|---|---|
| `YELLOW` | Dígitos con la barra de tareas oscura | amarillo |
| `AMBER` | Dígitos con la barra de tareas clara | ámbar oscuro |
| `RED` | Batería baja | rojo |
| `GREEN` | Cargando | verde |
| `LIGHT_FG` / `DARK_FG` | Contornos y línea según el tema | blanco / casi negro |
| `HEADSET_OPACITY` | Ícono de auriculares sin dispositivo | 60 % |
| `TRACK_OPACITY` | Parte vacía de la línea de carga | 30 % |
| `APP_ICON_GRAY` | Ícono del `.exe` | gris medio |

Cuando no hay headset detectado, o el headset está apagado, se muestran **auriculares grises atenuados**.

### Vista previa de los íconos

`cargo test --release` dibuja todos los íconos en ambos estilos, con los dos temas, en los tamaños 16/20/24/32/40/48 px, más el ícono del `.exe`. El resultado se guarda en `target\icon_preview.bgra`: un encabezado de 8 bytes con el ancho y el alto (`u32` little-endian) seguido de píxeles BGRA. Sirve para revisar cambios de diseño sin tener el headset conectado.

## Cómo funciona

### Comunicación con el headset ([src/hid.rs](src/hid.rs))

1. Lista las interfaces HID presentes con `CM_Get_Device_Interface_ListW` y descarta enseguida las que no son Corsair (filtra `vid_1b1c` en la ruta), así nunca abre dispositivos ajenos.
2. Abre cada candidata, confirma el modelo con `HidD_GetAttributes` y obtiene el tamaño de los reportes con `HidP_GetCaps`.
3. Envía el pedido `[0xC9, 0x64]`, completado con ceros hasta el tamaño del reporte de salida. Se queda con la primera interfaz que lo acepta.
4. Lee los reportes de entrada con I/O asíncrona (`ReadFile` con `OVERLAPPED`): el byte 2 es la batería y el byte 4 el estado. Si el bit 7 de la batería está activo, indica el micrófono levantado y se descarta.

| Estado | Significado |
|---|---|
| 0 | Desconectado (headset apagado) |
| 1 | Conectado |
| 2 | Batería baja |
| 4 | Carga completa |
| 5 | Cargando |

### Loop principal ([src/main.rs](src/main.rs))

- **Un solo hilo propio y un solo proceso.** Una ventana oculta recibe los mensajes de Windows.
- `MsgWaitForMultipleObjectsEx` espera **a la vez** los mensajes de la ventana y el evento de la lectura HID: el programa no sondea nunca, solo se despierta cuando llega un dato o un mensaje.
- `RegisterDeviceNotificationW` avisa cuando un dispositivo HID se conecta o desconecta. Se espera 1 segundo antes de volver a buscar, porque el dongle registra varias interfaces una tras otra.
- El ícono solo se redibuja cuando cambia algo: el estado, el tema (`WM_SETTINGCHANGE`) o la escala (`WM_DPICHANGED`, `WM_DISPLAYCHANGE`).
- El mensaje `TaskbarCreated` vuelve a agregar el ícono si se reinicia el Explorador.
- Un mutex con nombre (`Local\corsair_battery_level`) impide que haya dos copias abiertas.

### Dibujo de los íconos ([src/draw.rs](src/draw.rs))

Las figuras se describen como *signed distance fields* (SDF): para cada píxel se calcula la distancia al borde de la figura, lo que da bordes suavizados a cualquier tamaño sin usar GDI+ ni Direct2D. Los bordes de la batería y de los dígitos se alinean a píxeles enteros para que se vean nítidos a 16 px. El resultado se convierte en un ícono de Windows con `CreateDIBSection` y `CreateIconIndirect` ([src/icon.rs](src/icon.rs)).

## Recursos que utiliza

Medido en Windows 11, release x64:

| Recurso | Valor |
|---|---|
| Tamaño del `.exe` | ~152 KB (incluye el ícono de ~5,6 KB) |
| Memoria privada | ~1,5 MB |
| Working set | ~9–10 MB (casi todo DLLs del sistema compartidas con otros procesos) |
| CPU al iniciar | ~0,03 s |
| CPU en reposo | 0 %: sin timers ni sondeo |
| Procesos | 1 |
| Archivos externos | Ninguno (no extrae nada a `%TEMP%` ni lee archivos de configuración) |

## Uso

- Ejecutá `corsair_battery_level.exe`. El ícono aparece en la bandeja del sistema.
- En Windows 11 los íconos nuevos suelen aparecer ocultos en el menú de la flecha **^**. Para dejarlo siempre visible, arrastralo a la barra de tareas o activalo en *Configuración → Personalización → Barra de tareas → Otros iconos de la bandeja del sistema*.
- **Para que inicie con Windows:** presioná `Win + R`, escribí `shell:startup` y poné ahí un acceso directo al `.exe`.

### Distribución

El `.exe` no está firmado digitalmente. En equipos con **Smart App Control** activo, Windows lo va a bloquear. Para distribuirlo sin ese problema hay que firmarlo con un certificado de confianza, por ejemplo con Azure Trusted Signing.
