# Especificación — Stade Studio

**Estado:** aplicación local en evolución · **Alcance:** Windows, un operador · **Actualizada:** 2026-09-29

## 1. Propósito

Stade Studio registra carpetas de proyectos y permite coordinar tareas de OpenCode con un **Builder por proyecto**. La prioridad es una UI sencilla; procesos, worktrees, persistencia y MCP son infraestructura interna.

OpenCode Go es el plan del usuario, no el runtime de esta aplicación. Se usa la autenticación y configuración OpenCode existentes; no se duplican credenciales de proveedores.

## 2. Modelo de producto

- **Proyecto:** carpeta local registrada. Un proyecto puede pertenecer a varios espacios o quedar sin espacio; registrar/eliminar un proyecto nunca borra su carpeta fuente.
- **Espacio:** agrupación opcional muchos-a-muchos para filtrar proyectos y lanzar sus tareas listas. No es requisito para crear proyectos.
- **Builder:** un perfil/modelo OpenCode asignado por proyecto, con instrucciones base editables y compartidas en **Perfiles**.
- **Tarea:** la crea el usuario, queda asociada a un proyecto y se dirige al Builder de ese proyecto. Puede tener dependencias dentro del mismo proyecto, entregas, bloqueos o preguntas al usuario.
- **Uso:** métricas existentes del servidor OpenCode conectado; no son la cuota global de OpenCode Go.

**Planner no está asignado a proyectos ni espacios en esta versión.** El CRUD y la publicación de planes por Planner quedan fuera de alcance para mantener el flujo simple. Las tareas se definen desde Tareas y el Builder las ejecuta.

## 3. Interfaz

Navegación primaria: **Proyectos, Tareas, Perfiles, Uso y OpenCode**.

- La carpeta de proyecto se registra sin necesidad de crear un espacio.
- Espacios se gestionan desde Proyectos y filtran tanto Proyectos como Tareas.
- Perfiles contiene las instrucciones base del Builder.
- Configurar perfil/modelo se hace una vez por proyecto; la creación de tarea solo pide título y objetivo.
- Worktrees, ramas, sesiones y diagnóstico MCP permanecen en controles avanzados.

## 4. Flujo de trabajo

1. Conectar OpenCode o iniciarlo en segundo plano.
2. Registrar una carpeta; añadirla a un espacio si se desea.
3. Asignar Builder y editar sus instrucciones base.
4. Añadir tareas para el proyecto y lanzar las listas.
5. Revisar entregas, responder preguntas o atender bloqueos.

Una tarea dependiente solo queda lista cuando sus dependencias están completadas y la entrega correspondiente fue aceptada. Cada tarea de código mantiene su worktree Git administrado fuera del checkout principal. Un worktree no es sandbox de Windows.

## 5. MCP y límites de confianza

El servidor MCP lo sirve la aplicación en `127.0.0.1` con puerto efímero y token rotado por ejecución. La configuración generada vive bajo los datos locales de Stade Studio, nunca en el repositorio del usuario. Se rechazan peticiones con `Origin` y no se exponen tokens por IPC/UI/errores.

OpenCode adjunta el `sessionID` en `_meta`; Stade Studio deriva de él la tarea activa. Las herramientas Builder son `get_task_context`, `submit_handoff`, `complete_task`, `report_blocker` y `request_user_input`. No aceptan IDs arbitrarios de proyecto/tarea para ampliar permisos. Las llamadas son durables en SQLite; prompts y respuestas completos permanecen en OpenCode por defecto.

El SSE de OpenCode es volátil: se usa como invalidación y se reconcilia el estado consultando sesiones; no se retransmiten payloads sensibles a la GUI. Las credenciales OpenCode se mantienen en memoria y la app solo detiene procesos que inició.

## 6. Arquitectura

- Windows desktop: Tauri 2 + React/TypeScript/Vite; GUI ↔ Rust por IPC.
- Núcleo local: Rust/Tokio, cliente HTTP/SSE OpenCode y procesos locales.
- Persistencia: SQLite + SQLx en datos de aplicación.
- Sin servidor público, telemetría ni assets remotos. No se ejecuta shell arbitrario desde controles propios.
- Validar rutas, tipo de contenido, versión y contrato `openapi.json` del binario OpenCode conectado. OpenCode v2 usa rutas `/api/`; no asumir rutas v1.

Worktrees protegen el checkout principal, pero cualquier aislamiento adicional de Windows requeriría una solución separada y pruebas antes de prometerlo.

## 7. Eliminación de proyectos

Eliminar un proyecto borra su registro, tareas e historial local de coordinación, pero conserva la carpeta fuente, ramas Git y conversaciones OpenCode. Stade Studio retira solo worktrees limpios. Tareas activas o cambios sin guardar bloquean la operación; no se fuerzan eliminaciones de archivos.

## 8. Criterios de aceptación

1. Crear un proyecto con carpeta local, asignarlo a cero o varios espacios y filtrarlo desde Proyectos/Tareas.
2. Configurar un Builder por proyecto y editar instrucciones base reutilizables.
3. Crear tareas manuales, ejecutarlas con el Builder correcto y revisar entregas/bloqueos.
4. Rechazar llamadas MCP sin token, con `Origin`, sesión desconocida o fuera de la tarea activa.
5. Mantener coordinación y tareas en SQLite tras reinicios sin persistir prompts completos.
6. Eliminar proyectos sin tocar carpetas fuente ni datos sin guardar; bloquear el borrado si hay trabajo activo o worktrees sucios.
7. Conservar la página Uso y aclarar que no muestra cuota global de OpenCode Go.

## 9. Estado

- Implementado: proyectos (incluidos proyectos sin espacio), espacios opcionales, Uso, Builder por proyecto, instrucciones base, tareas, handoffs/decisiones, coordinación MCP local y worktrees por tarea.
- Simplificado: se retiraron Planner/perfil Planner, publicación de planes por MCP, Planner por proyecto/espacio, roles arbitrarios, recomendaciones/fallbacks y timeline global.
- Migración 0014: elimina asignaciones Planner y la tabla experimental de publicaciones; conserva Builders, espacios opcionales y tareas existentes. Las migraciones previas no se reescriben.
- Pendiente: validación visual E2E de Proyecto → Builder → tarea → entrega/decisión, reinicio de OpenCode y pérdida de SSE.

## 10. Pendiente tras el próximo reset de créditos

Evaluar la evolución a **Planner por espacio + Builder por proyecto**, sin hacer obligatoria la pertenencia a un espacio:

- Un Planner podría planificar varios proyectos de un espacio y publicar un lote de tareas dirigido a distintos proyectos.
- Cada tarea conservaría su proyecto destino y heredaría el Builder de ese proyecto; las dependencias/handoffs podrían cruzar proyectos dentro del mismo plan.
- El MCP tendría que vincular la sesión Planner al espacio y limitar los destinos a proyectos miembros, resolviendo cada Builder en el servidor sin confiar en IDs arbitrarios del modelo.
- La UI podría ofrecer **Planificar** desde un proyecto (alcance de un proyecto) o desde un espacio (alcance de varios), manteniendo proyectos sin espacio y tareas manuales.
- Revisar el modelo de tareas/dependencias y la vista de Tareas antes de implementar; no reactivar la publicación Planner actual por proyecto.

## 11. Comprobaciones

```powershell
# apps/desktop
npm run typecheck
npm run build

# apps/desktop/src-tauri
cargo fmt --check
cargo test
```
