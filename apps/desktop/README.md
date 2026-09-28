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

La contraseña solo se mantiene en memoria. En **Proyectos** puedes registrar, editar y archivar proyectos, crear worktrees Git independientes bajo el directorio de datos de la app (fuera del repositorio), asociar una sesión OpenCode en esa ruta y enviarle una tarea explícita. Para crear un worktree, el proyecto debe tener un commit y estar limpio; los cambios locales sin seguimiento también bloquean la operación. La app muestra el estado/respuesta al actualizar y ofrece aprobar permisos solo una vez o rechazarlos. No guarda prompts en SQLite. Un worktree separa archivos, pero no es un sandbox de Windows.

## Comprobaciones

```powershell
npm run typecheck
npm run build
cd src-tauri
cargo test
```

La GUI usa IPC de Tauri. No se carga contenido externo ni se expone el backend a la red.
