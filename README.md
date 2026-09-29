# Stade Studio

Aplicación comunitaria de escritorio local para coordinar proyectos, agentes y sesiones de OpenCode desde una sola interfaz.

- Especificación: [`docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md`](docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md)
- Instrucciones para agentes de código: [`AGENTS.md`](AGENTS.md)
- Aplicación Tauri: [`apps/desktop`](apps/desktop)

## Desarrollo local

1. Instala Node.js, Rust, Microsoft C++ Build Tools y WebView2 en Windows.
2. Desde `apps/desktop`, ejecuta `npm install` y `npm run tauri dev`.
3. En el panel de Stade Studio, pulsa **Iniciar y conectar** para arrancar OpenCode en segundo plano y conectar automáticamente. También puedes conectarte a un servidor local que ya esté iniciado.

La app permite iniciar/detener el servidor OpenCode que ella misma lanzó, registrar proyectos, preparar worktrees Git independientes, crear una sesión OpenCode por entorno y enviar tareas explícitamente. También muestra respuestas recientes y solicitudes de permisos; las aprobaciones son de una sola vez. Las sesiones cargadas se actualizan mediante SSE con reconexión y reconciliación; si el flujo se interrumpe, la interfaz avisa que el estado puede estar desactualizado. La página **Uso** muestra métricas del servidor conectado, no la cuota global de la cuenta; para esa cuota enlaza a OpenCode Console.

La página **Tareas** añade coordinación durable: plan con dependencias, una tarea por worktree, entregas revisables, preguntas al usuario y un puente MCP local con token por ejecución, cuya configuración se escribe junto a los worktrees administrados y nunca dentro de tu repositorio. La integración revisable de cambios sigue pendiente.

Los **Espacios** agrupan proyectos que trabajan juntos (p. ej. base de datos, backend y frontend de una app); un proyecto compartido puede vivir en varios espacios. Filtran proyectos y tareas y permiten lanzar las tareas listas de todo el espacio. La timeline combinada por espacio sigue pendiente.

Cada entorno Git (**Revisar cambios**) compara su rama contra el checkout principal y permite integrarla con un merge explícito y revisable, protegiendo cambios locales y abortando ante conflictos.
