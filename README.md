# OpenCode Conductor

Aplicación comunitaria de escritorio local para coordinar proyectos, agentes y sesiones de OpenCode desde una sola interfaz.

- Especificación: [`docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md`](docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md)
- Instrucciones para agentes de código: [`AGENTS.md`](AGENTS.md)
- Aplicación Tauri: [`apps/desktop`](apps/desktop)

## Primer prototipo

1. Instala Node.js, Rust, Microsoft C++ Build Tools y WebView2 en Windows.
2. Desde `apps/desktop`, ejecuta `npm install` y `npm run tauri dev`.
3. Inicia `opencode serve --hostname 127.0.0.1 --port 4096`, copia la contraseña temporal que muestra OpenCode y comprueba la conexión desde la ventana (usuario `opencode`).

El primer corte solo verifica la ventana de escritorio y la conexión local autenticada a OpenCode; el gestor de procesos, proyectos y coordinación de tareas se incorporarán en las siguientes iteraciones.
