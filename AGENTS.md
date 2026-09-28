# AGENTS.md — Stade Studio

## Propósito

Este repositorio desarrolla **Stade Studio**, una aplicación de escritorio **local** para coordinar proyectos, sesiones, perfiles y modelos de OpenCode desde una sola GUI. Es un proyecto comunitario, no oficial. No es un servicio cloud ni un panel web para VPS en el alcance actual.

La especificación funcional y los criterios de aceptación están en [`docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md`](docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md). Consúltala antes de cambiar el alcance.

## Stack acordado

- Tauri 2 para escritorio.
- React + TypeScript + Vite para la GUI.
- Rust para integración local, procesos, OpenCode y coordinación.
- SQLite + SQLx para persistencia local en el directorio de datos de la aplicación.
- Windows es el único sistema operativo objetivo en esta etapa; no invertir esfuerzo en compatibilidad con macOS/Linux.
- Identidad visual: Stade Studio, estilo cozy y minimalista, paleta pastel basada en `#ff3333`, `#ffb433`, `#333395` y grises cálidos. Mantener consistentes el logo y los recursos de Tauri.
- OpenCode Go es el plan/suscripción de acceso a modelos del usuario, no el nombre ni el runtime de esta aplicación. Usar la configuración OpenCode existente; no duplicar credenciales de proveedores.

## Hallazgos de integración OpenCode

- En el entorno inicial se verificó **OpenCode v2.0.16** y después **v2.0.18**. La API v2 probada usa `GET /api/info`; `/api/` es el prefijo de la API y `GET /openapi.json` sirve el contrato de la versión instalada.
- No asumir que las rutas de documentación v1, como `/global/health`, aplican a v2. En algunas rutas desconocidas el servidor puede devolver su SPA HTML con estado 200. Comprobar `Content-Type`, versión y OpenAPI; no interpretar HTML como respuesta API.
- El servidor v2 de prueba anunció una contraseña temporal al iniciarse y protegió la API con Basic Auth. Si la aplicación inicia el proceso, debe capturar esa credencial en memoria. Nunca imprimirla en logs, eventos, errores, SQLite o archivos de configuración ni mostrarla sin enmascarar en la UI. Al conectarse a un proceso externo, pedir la credencial sin persistirla por defecto.
- Limitar conexiones a `localhost`/loopback en el MVP. Rechazar URLs externas, credenciales embebidas en URL y rutas inesperadas salvo que exista un requisito aprobado.
- Validar los endpoints y eventos contra `openapi.json` del binario instalado; las páginas generales pueden documentar rutas de otra versión.

## Arquitectura y seguridad

- La GUI se comunica con Rust usando IPC/Channels de Tauri; no introducir un servidor web público para la interfaz.
- Si las custom tools/MCP necesitan hablar con el coordinador, el puente debe ser local, autenticado con capacidad de alcance mínimo y ligado a la sesión/tarea real. No confiar en IDs de proyecto/tarea arbitrarios que aporte el modelo.
- Los handoffs, bloqueos y preguntas deben ser durables y gestionados por el orquestador. No asumir que distintas sesiones/modelos comparten memoria.
- Persistir SQLite y los worktrees administrados fuera de los repositorios, bajo directorios de datos de la aplicación. No duplicar prompts/respuestas completos por defecto.
- No habilitar ejecución de shell arbitraria desde controles propios. Mantener permisos de herramientas visibles y auditables.
- No enviar telemetría ni cargar fuentes, scripts o assets remotos; la aplicación debe funcionar offline salvo las llamadas a modelos/servicios de OpenCode.
- No registrar prompts, respuestas completas, contraseñas, tokens ni contenido sensible por defecto.

## Organización del código

- Aplicación de escritorio: `apps/desktop`.
- Especificación: `docs/ESPECIFICACION-ORQUESTADOR-LOCAL-OPENCODE.md`.
- Mantener módulos Rust separados por dominio/integración al crecer; no concentrar toda la lógica en comandos Tauri.
- Mantener tipos de IPC explícitos y errores útiles para la UI. Las credenciales no deben formar parte de errores serializados.
- No usar `unwrap()`/`expect()` en rutas de ejecución de usuario salvo inicialización irrecuperable claramente justificada.

## Comprobaciones

Desde `apps/desktop`:

```powershell
npm install
npm run typecheck
npm run build
npm run tauri dev
npm run tauri -- build --no-bundle
```

Desde `apps/desktop/src-tauri`:

```powershell
cargo fmt --check
cargo test
```

Al modificar el cliente OpenCode, probar errores de conexión, Basic Auth, respuestas HTML/JSON y la versión OpenCode concreta disponible. Al modificar procesos, comprobar que al cerrar la aplicación solo se detengan los procesos que ella inició.

## Higiene de Git

- Versionar fuentes, documentación, `package-lock.json` y `Cargo.lock`.
- No versionar dependencias instaladas, builds, esquemas generados, bases de datos locales, secretos ni logs. Las reglas están en el `.gitignore` raíz y en `apps/desktop/.gitignore`.
- No borrar/revertir cambios de usuario ni ejecutar comandos destructivos de Git sin autorización explícita.
