# Stade Studio

Aplicación de escritorio local para coordinar proyectos OpenCode con un Builder asignado por proyecto.

- **Proyectos:** carpetas locales; pueden pertenecer a uno o varios espacios o quedar sin espacio.
- **Espacios:** agrupaciones opcionales para filtrar proyectos y lanzar sus tareas listas.
- **Perfiles:** instrucciones base editables del Builder.
- **Tareas:** las añade el usuario y se asignan al Builder configurado para el proyecto.
- **Uso:** métricas existentes del servidor OpenCode conectado.

La UI se centra en Proyectos, Tareas, Perfiles, Uso y conexión OpenCode. El MCP local coordina el Builder (contexto, entregas, bloqueos y decisiones); no hay Planner asignado a proyectos o espacios en esta versión. Los detalles Git/worktree y diagnóstico MCP quedan en controles avanzados.

Eliminar un proyecto lo quita de Stade Studio y borra sus datos de coordinación, pero conserva la carpeta original y sus ramas Git. Tareas activas o cambios sin guardar en worktrees bloquean la eliminación.

## Desarrollo local

En Windows instala Node.js, Rust, Microsoft C++ Build Tools y WebView2. Desde `apps/desktop`:

```powershell
npm install
npm run tauri dev
```

Comprobaciones:

```powershell
npm run typecheck
npm run build
cd src-tauri
cargo fmt --check
cargo test
```

Consulta [`docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md`](docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md) para alcance y seguridad.
