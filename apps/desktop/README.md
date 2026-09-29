# Stade Studio

Aplicación de escritorio local basada en Tauri 2, React/TypeScript y Rust.

## Desarrollo

Desde `apps/desktop`:

```powershell
npm install
npm run tauri dev
```

En el panel principal, pulsa **Iniciar y conectar** para lanzar OpenCode en segundo plano. Stade Studio captura la contraseña temporal, valida el servidor y carga el catálogo automáticamente. También puedes conectar un servidor que hayas iniciado por separado:

```powershell
opencode serve --hostname 127.0.0.1 --port 4096
```

Para la conexión manual, OpenCode v2 muestra una contraseña temporal en la salida de `serve`; usa `http://127.0.0.1:4096`, el usuario `opencode` y esa contraseña. La app valida la conexión con `/api/info` y después intenta descubrir perfiles (`/api/agent`) y modelos (`/api/model`). Si una ruta no está disponible en la versión conectada, la interfaz muestra un aviso y conserva el resultado parcial. Al cerrar Stade Studio, solo se detiene el servidor que inició la propia app.

La contraseña solo se mantiene en memoria. En **Proyectos** puedes registrar, editar y archivar proyectos, crear worktrees Git independientes bajo el directorio de datos de la app (fuera del repositorio), asociar una sesión OpenCode en esa ruta y enviarle una tarea explícita. Para crear un worktree, el proyecto debe tener un commit y estar limpio; los cambios locales sin seguimiento también bloquean la operación. Las sesiones cargadas se actualizan por SSE con reconexión y reconciliación; también se conserva el refresco manual. La app ofrece aprobar permisos solo una vez o rechazarlos. No guarda prompts en SQLite. Un worktree separa archivos, pero no es un sandbox de Windows.

En **Uso** la app muestra gasto y tokens registrados por el servidor conectado, con detalle por modelo para hoy, 5 horas, 7 días y mes calendario local. Estas métricas no son la cuota global de OpenCode Go; la API local no la expone y la fuente oficial es OpenCode Console, disponible desde el botón de la página. No se consultan endpoints privados de Console ni se guardan sus claves.

## Tareas y coordinación

La página **Tareas** guarda en SQLite el plan del proyecto: tareas con estados (`pending`, `ready`, `working`, `blocked`, `review`, `completed`, `failed`), dependencias, entregas y decisiones de usuario. Una tarea dependiente solo pasa a **lista** cuando todas sus dependencias están completadas y con entrega aceptada; un worktree no puede sostener dos tareas activas.

Al lanzar una tarea, Stade Studio:

1. crea su worktree aislado (o reutiliza el de la tarea) y escribe `opencode.json` en el directorio administrado `worktrees/project-<id>/`, fuera del repositorio, registrando el servidor MCP `stade`;
2. crea la sesión OpenCode en la ruta exacta del worktree con el perfil y el modelo de la tarea;
3. envía objetivo, ámbito de archivos y entregas aceptadas de sus dependencias. El texto del prompt no se guarda.

El agente dispone de `stade_get_task_context`, `stade_submit_handoff`, `stade_complete_task`, `stade_report_blocker` y `stade_request_user_input`. El puente escucha solo en `127.0.0.1`, con un token nuevo por ejecución y sin CORS; la identidad se resuelve con el `sessionID` que OpenCode adjunta a cada llamada, no con lo que diga el modelo. Aceptar una entrega desbloquea a las tareas dependientes; devolverla con nota vuelve a la sesión como mensaje nuevo.

Si reinicias la app con el servidor OpenCode ya abierto, reinicia también el servidor o usa **Regenerar configuración** en la página Tareas para que tome el token vigente.

## Roles

La página **Roles** guarda especialidades reutilizables: nombre, descripción, instrucciones (llegan al agente en cada tarea del rol), perfil OpenCode, modelo preferido y alternativo, ámbito de archivos y palabras clave. Al crear una tarea puedes elegir rol (rellena perfil, modelo y ámbito, todo editable) o pulsar **Sugerir rol**, que propone hasta 3 candidatos con motivos legibles —palabras en título/objetivo y solapamiento de ámbito— sin puntuación de calidad. Borrar un rol no afecta a las tareas existentes.

El modelo alternativo nunca se aplica solo: en tareas listas o pendientes con rol verás **Con alternativo**, que cambia el modelo de la tarea (con confirmación) antes de crear su sesión. Si la tarea ya tiene sesión, conserva su modelo.

## Espacios

Los **Espacios** agrupan proyectos que trabajan juntos (p. ej. "App finanzas" con base de datos, backend y frontend). La relación es de muchos a muchos: un backend compartido puede estar en dos espacios a la vez. El espacio es solo una lente de agrupación y acción: los worktrees, sesiones, tareas y `opencode.json` siguen perteneciendo a cada proyecto, y el puente MCP no cambia.

En **Proyectos** puedes crear, renombrar, archivar y eliminar espacios (eliminar solo borra la agrupación, nunca los proyectos), filtrar la lista por espacio y asignar cada proyecto con casillas. En **Tareas** el selector de espacio filtra el proyecto y el botón **Lanzar espacio** (con confirmación) lanza las tareas listas de todos sus proyectos, agregando los resultados.

## Integración revisable

Cada entorno con estado listo ofrece **Revisar cambios**: muestra la rama frente a la rama activa del checkout principal (commits de adelanto/retraso, archivos y diff recortado) y el botón **Integrar** hace un merge explícito `--no-commit --no-ff` seguido de commit. Si algún lado tiene cambios sin guardar, se bloquea; si hay conflictos, se aborta sin tocar tu rama y se listan los archivos en conflicto. El entorno se conserva tras integrar por si necesitas volver a su rama.

## Comprobaciones

```powershell
npm run typecheck
npm run build
cd src-tauri
cargo test
```

La GUI usa IPC de Tauri. No se carga contenido externo ni se expone el backend a la red.
