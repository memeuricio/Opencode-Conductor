# Stade Studio — aplicación de escritorio

Stade Studio coordina proyectos locales de OpenCode. Cada proyecto puede tener un **Builder** (perfil/modelo OpenCode y sus instrucciones base); los espacios son opcionales y solo agrupan proyectos.

## Flujo

1. Conecta OpenCode (preferentemente **Iniciar y conectar**) y registra una carpeta de proyecto.
2. Opcionalmente agrupa el proyecto en uno o varios espacios.
3. Asigna un Builder al proyecto y ajusta sus instrucciones base en **Perfiles**.
4. Añade tareas en **Tareas**, revisa bloqueos/entregas y lanza el trabajo.

No hay Planner asignado a proyectos o espacios en esta versión; el usuario define las tareas. **Uso** conserva las métricas del servidor OpenCode conectado y no representa la cuota global de OpenCode Go.

## Coordinación MCP

El puente local usa Streamable HTTP en loopback con token por ejecución. OpenCode adjunta el `sessionID` y Stade Studio lo resuelve a una tarea activa; el modelo no elige proyecto ni tarea mediante IDs arbitrarios. El Builder puede consultar contexto, entregar/completar trabajo, reportar bloqueos y pedir decisiones.

Las tareas, dependencias y entregas se guardan en SQLite; las conversaciones permanecen en OpenCode. La configuración MCP se escribe fuera de los repositorios. Los controles de reparación y worktrees están bajo **Avanzado**. Los worktrees separan cambios, pero no son un sandbox de Windows.

Eliminar un proyecto conserva la carpeta fuente y sus ramas/conversaciones OpenCode. Se eliminan los registros locales y solo los worktrees limpios; tareas activas o cambios sin guardar bloquean el borrado.

## Desarrollo y pruebas

Desde `apps/desktop`:

```powershell
npm install
npm run typecheck
npm run build
npm run tauri dev
```

Desde `apps/desktop/src-tauri`:

```powershell
cargo fmt --check
cargo test
```

La migración 0014 elimina las asignaciones Planner y las publicaciones Planner experimentales; conserva Builders, proyectos huérfanos y tareas ya guardadas. Las tablas/migraciones históricas no se reescriben para preservar compatibilidad.
