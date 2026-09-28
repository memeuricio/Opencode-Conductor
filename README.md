# Stade Studio

Aplicación comunitaria de escritorio local para coordinar proyectos, agentes y sesiones de OpenCode desde una sola interfaz.

- Especificación: [`docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md`](docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md)
- Instrucciones para agentes de código: [`AGENTS.md`](AGENTS.md)
- Aplicación Tauri: [`apps/desktop`](apps/desktop)

## Desarrollo local

1. Instala Node.js, Rust, Microsoft C++ Build Tools y WebView2 en Windows.
2. Desde `apps/desktop`, ejecuta `npm install` y `npm run tauri dev`.
3. En el panel de Stade Studio, pulsa **Iniciar y conectar** para arrancar OpenCode en segundo plano y conectar automáticamente. También puedes conectarte a un servidor local que ya esté iniciado.

La app permite iniciar/detener el servidor OpenCode que ella misma lanzó, registrar proyectos, preparar worktrees Git independientes, crear una sesión OpenCode por entorno y enviar tareas explícitamente. También muestra respuestas recientes y solicitudes de permisos; las aprobaciones son de una sola vez. Las sesiones cargadas se actualizan mediante SSE con reconexión y reconciliación; si el flujo se interrumpe, la interfaz avisa que el estado puede estar desactualizado. La página **Uso** muestra métricas del servidor conectado, no la cuota global de la cuenta; para esa cuota enlaza a OpenCode Console. La coordinación durable mediante handoffs sigue pendiente.
