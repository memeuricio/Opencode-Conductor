# Especificación — orquestador local de agentes OpenCode

**Nombre del producto:** OpenCode Conductor  
**Repositorio sugerido:** `opencode-conductor` (proyecto comunitario, no oficial)

**Estado:** desarrollo iniciado; primera prueba vertical en progreso  
**Versión:** 0.3  
**Fecha:** 2026-09-25  
**Alcance confirmado:** aplicación local para un operador; el usuario tiene una suscripción/plan OpenCode Go para acceder a modelos. OpenCode Go no es el nombre de esta aplicación ni un runtime que esta deba ejecutar.

## 1. Objetivo

Construir una aplicación de escritorio local desde la que el usuario pueda administrar varios proyectos y sesiones OpenCode, asignar modelos/agentes por especialidad, coordinar tareas entre ellos y recibir sus avances, bloqueos y solicitudes de decisión, sin tener una consola de terminal abierta por proyecto.

El MVP corre en el equipo del usuario y se conecta a procesos `opencode serve` locales. La aplicación puede iniciar/administrar esos procesos en segundo plano para ocultar las consolas, además de conectarse a un servidor que el usuario ya haya iniciado. No requiere VPS, cuentas multiusuario ni conectores remotos. El soporte de VPS/múltiples equipos queda para una rama o fase separada.

## 2. Decisiones y aclaraciones de producto

1. **“Agente” tiene dos significados que no deben mezclarse:**
   - **Perfil de agente OpenCode:** una configuración/rol descubierto desde OpenCode (por ejemplo, un agente primario o subagente).
   - **Trabajo activo:** una sesión o ejecución que usa ese perfil.
   La interfaz debe mostrar perfiles y sesiones; un perfil puede tener más de una sesión.
2. **La coordinación entre agentes pertenece a la plataforma.** No se debe confiar en que dos modelos compartan contexto o se notifiquen por sí solos. El orquestador guarda tareas, dependencias, entregables, mensajes de traspaso y preguntas en estado persistente.
3. **“Emitir eventos” significa comunicación operativa:** recibir el SSE de OpenCode, mostrar actividad en la interfaz, enviar tareas/prompts a las sesiones y publicar mensajes/traspasos durables para el siguiente agente. OpenCode documenta SSE de entrada y APIs de sesión para acciones salientes, no un endpoint para publicar eventos arbitrarios en su bus.
4. El enrutamiento inicial de tareas se basa en roles, capacidades y preferencias de modelo configuradas por el usuario. La aplicación puede recomendar un rol/modelo, pero no afirmará conocer objetivamente cuál modelo es “mejor” sin una evaluación configurable.
5. Los modelos y la autenticación se consultan/usan desde la configuración OpenCode existente, incluido OpenCode Go. La aplicación no gestiona la suscripción ni almacena credenciales de proveedor.
6. El MVP es local y monousuario. Se evitan abstracciones multiusuario y despliegue cloud hasta validar el flujo de trabajo.

## 3. Stack elegido (cerrado)

| Capa | Elección recomendada | Motivo |
|---|---|---|
| Aplicación de escritorio | **Tauri 2** | GUI de escritorio nativa y ligera, con Rust limitado a la capa local. No se abrirá una consola web como interfaz principal. |
| Frontend | **React + TypeScript + Vite**; TanStack Query | UI para proyectos, tareas, agentes y timeline; comunicación con el backend mediante IPC de Tauri. |
| Núcleo local | **Rust dentro de Tauri** | Orquestación de tareas, procesos OpenCode, llamadas HTTP/SSE, seguridad local y persistencia. Evita empaquetar un segundo runtime Node como sidecar. |
| Cliente OpenCode | API HTTP/OpenAPI oficial mediante `reqwest` + tipos `serde` y cliente encapsulado | OpenCode expone una API neutral a lenguaje. No se depende de que el SDK TypeScript oficial corra dentro de Rust; validar endpoints/eventos con la versión elegida. |
| Persistencia | **SQLite + SQLx** | Adecuado para un operador local; guarda proyectos, tareas, dependencias, traspasos, decisiones y eventos sin servidor externo. Migraciones versionadas. |
| Procesos | Tokio `Command` / gestión de hijos desde Rust | Lanzar `opencode serve` en segundo plano y terminar únicamente procesos iniciados por la aplicación. |
| Tiempo real | SSE desde OpenCode; eventos/Channels de Tauri hacia React | Sin servidor web ni WebSocket público. |
| Comunicación agente-orquestador | Custom tools de OpenCode o MCP local; elegir tras una prueba técnica | Las herramientas deben entregar handoffs y solicitudes estructuradas y asociar cada llamada a su sesión/tarea. Si hace falta, el núcleo Rust expone un puente loopback autenticado solo para esos callbacks; la GUI sigue usando IPC Tauri. |
| Cola/broker | Ninguno | Cola durable en SQLite; no hacen falta Redis/NATS para un operador local. |
| Empaquetado | Instalador Tauri por sistema operativo | Incluir UI y backend en una aplicación local; no Docker ni VPS para el MVP. |

**Decisión Tauri vs Electron:** Tauri es la opción elegida porque ya tienes experiencia con él, reduce consumo/empaquetado frente a Electron y encaja con una aplicación local. El frontend seguirá en TypeScript; Rust se concentra en el núcleo del escritorio. Electron solo se reconsideraría si una prueba técnica demuestra un bloqueo concreto en Tauri/Rust, no por anticipación. Bun no es necesario: OpenCode se integra por su API HTTP y la app no debe compartir runtime con OpenCode.

### Verificación específica de OpenCode v2

- En el entorno de desarrollo se detectó OpenCode **v2.0.16**. El API probado expone información en `GET /api/info` y los endpoints v2 están bajo `/api/`; no se debe asumir que la ruta v1 `GET /global/health` sea válida para esta versión (puede devolver la SPA HTML).
- El API requiere Basic Auth cuando el servidor se inicia con contraseña. En la prueba local v2 anunció una contraseña temporal en stdout aunque no había una variable de contraseña en el entorno. La aplicación debe capturar la salida del proceso administrado y mantener la credencial solo en memoria; al conectarse a un proceso externo, pedir usuario/contraseña sin persistirlos por defecto.
- El contrato se debe contrastar con `GET /openapi.json` de la versión ejecutada. Las rutas y la generación de contraseña pueden cambiar; no copiar ciegamente ejemplos v1 de documentación general.

## 4. Arquitectura propuesta

```text
Aplicación Tauri (ventana de escritorio)
  ├─ React/TypeScript ── IPC/Channels ── Rust core
  ├─ coordinador de tareas
  ├─ puente interno local autenticado ◄── custom tools/MCP de agentes
  ├─ SQLite (cola, dependencias, entregables, eventos y auditoría)
  └─ gestor de procesos OpenCode (principal; conexión a proceso existente opcional)
            │ HTTP/SSE local
            ▼
OpenCode server(s) locales, uno por entorno de proyecto
  ├─ API HTTP (salud, perfiles, sesiones, acciones)
  └─ stream SSE de eventos
            │ cliente Rust basado en API/OpenAPI; reconexión + sincronización
            └──────────────► coordinador / dashboard
```

El coordinador asigna tareas a perfiles/modelos configurados, crea o utiliza sesiones, observa su estado y captura resultados. Los agentes necesitan herramientas invocables desde su sesión —por ejemplo `get_task_context`, `submit_handoff`, `complete_task`, `request_user_input` y `report_blocker`— que llamen al coordinador; no es comunicación directa entre modelos. La integración puede ser mediante custom tools de OpenCode o un servidor MCP local. Debe validarse en la fase 0 cuál mecanismo puede asociar cada llamada de forma fiable a su `sessionID` y tarea. Cada sesión solo puede actuar sobre su tarea asignada. Cuando una tarea termina, guarda el traspaso y desbloquea la siguiente tarea dependiente. Las tareas bloqueadas por decisiones del usuario esperan una respuesta explícita.

### Límites de confianza

- Los servidores OpenCode y cualquier puente interno de herramientas deben escuchar solo en loopback; el puente requiere token aleatorio de corta duración/permisos mínimos y no debe aceptar llamadas arbitrarias de páginas web. No exponerlos a LAN/Internet. Una futura versión remota necesitará una arquitectura de seguridad separada.
- La aplicación no copia credenciales OpenCode Go ni claves de proveedor; delega autenticación y selección de modelos a la instalación OpenCode.
- Iniciar/abortar sesiones y aprobar permisos son acciones visibles y auditables. No habilitar ejecución arbitraria de shell desde controles propios de la plataforma.
- Registrar metadatos de eventos por defecto; excluir texto de prompts/respuestas y secretos, salvo que una decisión posterior habilite expresamente contenido con políticas de retención.
- Coordinar escrituras al mismo repositorio: no lanzar tareas paralelas con ámbitos de archivos solapados salvo que el usuario lo permita. Guardar checkpoint Git antes de automatizar cambios; worktrees aislados pueden ser una iteración posterior.

## 5. Modelo de dominio mínimo

- **OpenCodeConnection:** URL/base del servidor, alias, versión, estado de conexión, último contacto y referencia a credenciales seguras.
- **Project:** nombre, descripción opcional, ruta raíz y conexión OpenCode asociada.
- **AgentProfile:** ID/nombre/descripción/tipo reportado por OpenCode, asociado a una conexión. No es una ejecución.
- **AgentRole:** responsabilidad del producto (frontend, backend, base de datos, revisión, planificación), instrucciones, capacidades, ámbitos de archivos y modelo preferido/fallback.
- **ModelOption:** proveedor/modelo disponible en OpenCode y estado de disponibilidad. No duplicar secretos ni asumir datos de cuota que la API no exponga.
- **RoutingRule:** clasificación de tarea/capacidades requeridas → rol y modelo preferido/fallback, con explicación y posibilidad de override del usuario.
- **ProjectAgentAssignment:** relación entre proyecto, rol y perfil/modelo seleccionado; preferencias y marcas de auditoría.
- **Session:** ID de OpenCode, proyecto, rol/perfil/modelo elegido, título, estado, marcas temporales, tarea asociada y conexión.
- **Task:** objetivo, proyecto, rol/modelo asignado, estado (`pendiente`, `lista`, `trabajando`, `bloqueada`, `revisión`, `completada`, `fallida`), dependencias, ámbito de archivos y sesión OpenCode asociada.
- **Handoff:** resumen del trabajo, artefactos/archivos/commit, contrato o instrucciones, destinatario, preguntas/bloqueos y estado de aceptación.
- **UserDecision:** pregunta dirigida al usuario, opciones/contexto, tarea bloqueada y decisión/respuesta registrada.
- **PlatformEvent:** ID interno, origen, tarea/proyecto/sesión opcionales, tipo normalizado, hora de origen/recepción, payload redactado, clave de deduplicación y versión del esquema.
- **AuditEntry:** actor, acción, destino, resultado y fecha; sin guardar secretos.

## 6. Requisitos funcionales

### Conexión e integración con OpenCode

- **RF-01 — Abrir el espacio local:** mostrar proyectos, roles y sesiones en una ventana de escritorio Tauri; no exigir una consola por agente ni exponer una interfaz web pública.
- **RF-02 — Gestionar servidores OpenCode:** detectar/validar el CLI y permitir iniciar `opencode serve` en segundo plano por proyecto o conectar un servidor existente. Mostrar PID/estado, versión, puerto y errores; terminar solo procesos que la aplicación haya iniciado.
- **RF-03 — Descubrir modelos y perfiles:** consultar agentes y modelos/proveedores disponibles a través de la API/OpenAPI de OpenCode, incluidas las opciones que el plan OpenCode Go expone en la instalación local. No solicitar ni duplicar claves de proveedor.
- **RF-04 — Capturar eventos:** suscribirse al SSE de OpenCode, recibir evento de conexión y eventos disponibles, asociarlos a proyecto/sesión, deduplicar y reconectar con backoff.
- **RF-05 — Sincronizar estado:** recuperar salud, sesiones y estados tras reinicio o pérdida de SSE; marcar datos obsoletos mientras no se haya reconciliado el estado.
- **RF-06 — Enviar acciones:** crear/seleccionar sesiones, enviar prompts a perfiles/modelos asignados y abortar sesiones. Registrar el resultado y no afirmar que se publicó un evento arbitrario en el bus de OpenCode.
- **RF-07 — Errores de integración:** mostrar CLI ausente, error de arranque, autenticación, versión no compatible y último contacto, sin bloquear los demás proyectos.

### Proyectos, roles y tareas

- **RF-08 — Administrar proyectos:** crear/editar/archivar proyecto con ruta raíz, repositorio Git y servidor OpenCode asociado.
- **RF-09 — Definir roles:** configurar especialidad, instrucciones, modelo preferido y alternativo, permisos y ámbito de archivos por rol. Incluir ejemplos frontend, backend, base de datos, planificador y revisor.
- **RF-10 — Enrutar tareas:** permitir asignar manualmente o aceptar una recomendación de rol/modelo basada en tipo de tarea, capacidades declaradas y preferencias del usuario. Mostrar por qué se recomienda; permitir cambiarlo antes de ejecutar. Considerar compatibilidad con herramientas/contexto y modelo fallback; no prometer puntuación objetiva automática de calidad ni datos de cuota que OpenCode no exponga.
- **RF-11 — Crear/planificar tareas:** crear tareas a mano o pedir a un rol planificador que las desglose en tareas con responsables, dependencias, entregables y criterios de aceptación. El usuario puede revisar el plan antes de iniciar trabajo paralelo.
- **RF-12 — Coordinar dependencias:** una tarea dependiente solo se activa cuando sus prerrequisitos producen un handoff aceptado. Ejemplo: el rol de base de datos entrega migración/esquema; backend confirma que entiende el contrato y construye API; frontend recibe contrato/API y comienza integración.
- **RF-13 — Herramientas de coordinación:** ofrecer al agente herramientas invocables desde la sesión para consultar contexto de tarea, entregar resultados, reportar bloqueos y pedir decisión del usuario. Probar custom tools OpenCode y MCP local; escoger el mecanismo que permita validar `sessionID`/tarea con seguridad. No confiar en IDs arbitrarios enviados por el modelo. Si no se puede asociar una llamada a su tarea, no habilitar automatización de handoffs.
- **RF-14 — Mensajes y handoffs durables:** los agentes dejan resumen, artefactos/archivos/commit, decisiones, contrato/instrucciones para el siguiente rol y preguntas, usando handoff estructurado. El coordinador entrega el handoff al siguiente prompt; no depende de memoria compartida entre sesiones.
- **RF-15 — Bloqueos y preguntas:** un agente puede marcar una tarea bloqueada y solicitar decisión/entrada del usuario con contexto y opciones. La tarea no continúa hasta que el usuario responda o reasigne; esa decisión se incluye en el contexto al reanudar.
- **RF-16 — Conflictos de edición:** registrar alcance de archivos por tarea y advertir/bloquear asignaciones paralelas con solapamiento, salvo aprobación explícita. Mostrar cambios/archivos afectados y permitir revisión antes de integrar. El alcance declarado es una coordinación, no un sandbox de seguridad.
- **RF-17 — Control de ejecución:** pausar, reanudar o abortar trabajo desde la interfaz; confirmar inicio de tareas que consumen recursos del plan.

### Visibilidad y eventos de la plataforma

- **RF-18 — Dashboard:** ver tareas por estado, agente/rol y proyecto; sesiones activas; dependencias; últimos handoffs; errores y decisiones pendientes. Indicar modelo seleccionado y última actualización.
- **RF-19 — Estados normalizados:** como mínimo `desconectado`, `inactivo`, `trabajando`, `esperando_aprobacion`, `bloqueado`, `completado`, `fallido` y `desconocido`; guardar también estado crudo de OpenCode.
- **RF-20 — Notificaciones:** destacar en la interfaz (y opcionalmente notificación de escritorio en fase posterior) una pregunta del agente, handoff listo, error o intervención requerida.
- **RF-21 — Historial de actividad:** línea de tiempo por proyecto que combine eventos OpenCode y eventos de orquestación; retención configurable y sin contenido sensible por defecto.

## 7. Requisitos no funcionales

- **RNF-01 Seguridad local:** limitar la UI a IPC Tauri; proteger el puente de herramientas con token de capacidad y loopback; no exponer API a la red; redacción de datos sensibles; auditar acciones de control.
- **RNF-02 Disponibilidad del estado:** tras reiniciar API/conexión, reconstruir el estado actual consultando OpenCode; detectar estado obsoleto y no fingir que un agente está activo.
- **RNF-03 Resiliencia:** guardar cola/dependencias en SQLite para sobrevivir reinicios; aislar conexiones, usar timeout/backoff y deduplicar eventos.
- **RNF-04 Consistencia:** usar IDs de OpenCode junto con IDs internos y guardar eventos/estado con timestamps UTC. Tolerar eventos fuera de orden.
- **RNF-05 Compatibilidad:** fijar una versión probada del servidor/OpenAPI; verificar la versión conectada y gestionar incompatibilidad con mensaje claro. No depender de endpoints experimentales para funciones básicas.
- **RNF-06 Observabilidad:** logs locales con correlación de proyecto/tarea/sesión; no registrar prompts ni secretos. Mostrar razones de reintento y fallo en UI.
- **RNF-07 Usabilidad:** dashboard muestra última actualización; eventos SSE obsoletos deben activar sincronización de recuperación y señal visual de datos desactualizados.
- **RNF-08 Pruebas:** pruebas unitarias de router, dependencias, handoffs y estados; integración contra OpenCode; E2E de proyecto → plan → tarea → handoff → siguiente rol → decisión del usuario.

## 8. Criterios de aceptación del MVP

1. El usuario abre una ventana Tauri única y puede ver varios proyectos sin abrir una terminal por cada uno.
2. Puede iniciar o conectar OpenCode para un proyecto, ver modelos y perfiles disponibles desde su propia instalación y enterarse de fallos del proceso.
3. Puede configurar roles y modelos preferidos/fallback, crear un plan con tareas dependientes y revisar el plan antes de ejecución.
4. En una prueba de flujo, el agente de base de datos usa una herramienta de coordinación para entregar esquema/migración; backend recibe ese handoff en su tarea y produce contrato/API; frontend recibe ese contrato y puede continuar. Cada llamada de herramienta queda autenticada y asociada a la tarea y sesión OpenCode correctas.
5. Si un agente necesita una decisión, la tarea queda bloqueada y la interfaz presenta pregunta/contexto; tras la respuesta, el agente puede continuar con esa respuesta en su contexto.
6. El usuario ve tareas, sesiones y eventos SSE en una vista; tras pérdida de conexión se reconcilia el estado y las tareas pendientes sobreviven al reinicio de la aplicación.
7. Se advierten conflictos de alcance de archivos antes de ejecutar trabajo paralelo que pueda sobrescribir cambios.
8. No se copian credenciales OpenCode Go ni se guardan prompts/respuestas por defecto; no se exponen servidores a la red.

## 9. Plan de entrega recomendado

1. **Fase 0 — prueba técnica:** compilar una ventana Tauri mínima; iniciar/terminar `opencode serve` sin consola; capturar de forma segura la contraseña generada; consultar modelos/perfiles y consumir SSE desde Rust; comprobar custom tool/MCP con identidad fiable de sesión y callback autenticado al coordinador.
2. **Fase 1 — escritorio unificado:** navegación Tauri, administración de proyectos, arranque/conexión OpenCode, modelos/perfiles, sesiones y actividad SSE. Primera meta: dejar de manejar varias consolas.
3. **Fase 2 — colaboración durable:** SQLite, roles especializados, tareas/dependencias, router configurable, puente de herramientas seguro, handoffs estructurados, timeline y solicitudes de decisión al usuario.
4. **Fase 3 — ejecución segura:** prompt/abort desde UI, control de permisos, ámbitos de archivos/conflictos, historial, resiliencia y pruebas del flujo DB → backend → frontend.
5. **Fase 4 — calidad de vida local:** notificaciones de escritorio, worktrees Git por agente, resumen/revisión de cambios y heurísticas de recomendación de modelos.
6. **Rama futura independiente — remoto/VPS:** autenticación multiusuario, conector outbound y seguridad entre máquinas. Fuera del MVP y no debe condicionar el diseño inicial salvo mantener un límite API razonable.

## 10. Dificultad y delegación a modelos

Escala: **1** = tarea pequeña y bien delimitada; **10** = alta ambigüedad, integración/riesgo crítico. La puntuación orienta la capacidad necesaria, no estima horas.

| Trabajo delegable | Dificultad | Perfil recomendado |
|---|---:|---|
| Shell Tauri, IPC seguro, React/TypeScript y empaquetado | 6/10 | Modelo competente; revisión de capacidades/permisos de Tauri. |
| Gestor Rust de procesos OpenCode en segundo plano | 7/10 | Modelo fuerte en Tokio/procesos y apagado seguro; revisar por SO. |
| SQLite/SQLx, migraciones, repositorios y CRUD local | 6/10 | Modelo competente en Rust; revisar concurrencia y migraciones. |
| Cliente OpenCode Rust, catálogo de modelos/agentes y sesiones | 7/10 | Modelo fuerte; API HTTP/SSE tipada y pruebas contra versión real. |
| SSE entrante, reconexión y sincronización | 8/10 | Modelo fuerte en backend/event-driven; revisión obligatoria. |
| Motor de tareas, estados y dependencias durables | 8/10 | Modelo fuerte; probar reinicios, fallos y dependencias circulares. |
| Enrutamiento rol/modelo y configuración de capacidades | 7/10 | Modelo fuerte; evitar afirmaciones de calidad sin evaluación. |
| Handoffs entre sesiones con artefactos/contexto entregado | 8/10 | Modelo fuerte en integración y diseño de protocolos. |
| Puente custom tools/MCP autenticado y acotado por sesión/tarea | 9/10 | Modelo muy fuerte en integración/seguridad; prueba técnica y revisión humana obligatorias. |
| Bloqueos, preguntas al usuario y reanudación | 7/10 | Modelo competente-fuerte; asegurar que la decisión llega al contexto correcto. |
| Dashboard local React con timeline y tablero | 6/10 | Modelo competente con diseño de estados definido. |
| Control de concurrencia/alcance de archivos y Git worktrees | 9/10 | Modelo muy fuerte; alta posibilidad de pérdida/conflicto de cambios. |
| Pruebas E2E del flujo DB → backend → frontend | 8/10 | Modelo fuerte; integrar con OpenCode/modelos reales y validar manualmente. |
| Rama remota/VPS y conectores seguros | 9/10 | Modelo muy fuerte en arquitectura/seguridad; revisión humana obligatoria. |
| MVP completo | 9/10 | No delegar como una única tarea; dividir por fases e interfaces. |

**Regla práctica de selección:** 1–3 puede ir a un modelo económico; 4–6 a un modelo competente de código; 7–8 a un modelo fuerte con tests y revisión; 9–10 al modelo más capaz disponible más revisión humana. No delegar tareas de seguridad o decisiones ambiguas como “implementa todo” sin interfaces y criterios de aceptación.

## 11. Decisiones pendientes de producto (con valores predeterminados propuestos)

El stack queda cerrado; estas son decisiones de comportamiento. Se proponen valores para poder empezar sin más diseño técnico:

1. **Sistema operativo:** validar primero Windows (entorno actual) y mantener el código portable para macOS/Linux. Confirmar si necesitas otro objetivo desde la primera versión.
2. **Autonomía:** el usuario revisa/aprueba el plan inicial; después las tareas dependientes se ejecutan automáticamente. Pausar y pedir opinión ante ambigüedad, error, permiso sensible o conflicto de archivos.
3. **Cambios de código:** permitir a los agentes editar el proyecto; ejecutar en serie tareas con dependencias y permitir paralelismo solo para ámbitos de archivos distintos. Advertir antes de tareas solapadas. Dejar worktrees aislados para una fase posterior.
4. **Modelo:** recomendar por rol/capacidad y preferencia configurada; el usuario puede override. No cambiar de modelo automáticamente tras fallos salvo que habilite explícitamente el fallback.
5. **Historial:** SQLite guarda estado, eventos, handoffs y resúmenes; las conversaciones completas permanecen en OpenCode y no se duplican por defecto.
6. **Arranque OpenCode:** la aplicación inicia y administra procesos `opencode serve` en segundo plano; también puede conectarse a un proceso existente si el usuario lo prefiere.
7. **Notificaciones:** preguntas/bloqueos aparecen en una bandeja/inbox dentro de la GUI; notificaciones del sistema operativo quedan opcionales.

Las decisiones 1–3 son las únicas que conviene confirmar antes de cerrar criterios de aceptación. Si no se especifica otra cosa, usar los valores predeterminados anteriores.

## 12. Referencias oficiales para los implementadores

- [OpenCode Server](https://opencode.ai/docs/server/) — `opencode serve`, autenticación, SSE (`GET /event`), sesiones, estados y agentes.
- [OpenCode SDK](https://opencode.ai/docs/sdk/) — referencia de métodos/esquemas; el backend Tauri consumirá HTTP/OpenAPI desde Rust, no este SDK directamente.
- [OpenCode MCP servers](https://opencode.ai/docs/mcp-servers/) — herramientas MCP disponibles para modelos OpenCode.
- [OpenCode Custom Tools](https://opencode.ai/docs/custom-tools/) — alternativa a MCP; revisar contexto de sesión disponible para validar handoffs.
- Las páginas oficiales indicaban actualización al **25 de septiembre de 2026** al redactar esta especificación. La prueba local con v2.0.16 encontró diferencias respecto a rutas citadas en las páginas generales; usar el OpenAPI servido por el binario objetivo como contrato definitivo.

## 13. Estado de la primera prueba vertical

- Scaffold Tauri 2 + React/TypeScript creado en `apps/desktop`.
- Rust valida direcciones loopback, usa Basic Auth en memoria y consulta `GET /api/info`; la interfaz muestra versión/errores.
- Verificaciones ejecutadas: `npm run typecheck`, `npm run build`, `cargo fmt --check`, `cargo test` (2 pruebas de validación URL) y `npm run tauri -- build --no-bundle` (Windows, release).
- Aún no implementado: arranque/gestión de OpenCode por la app, listado real de agentes/modelos, SSE, SQLite, coordinación de tareas y handoffs.
