import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./tasks.css";

export type TaskStatus =
  | "pending"
  | "ready"
  | "working"
  | "blocked"
  | "review"
  | "completed"
  | "failed";

export interface Task {
  id: number;
  projectId: number;
  worktreeId: number | null;
  title: string;
  objective: string;
  status: TaskStatus;
  agentId: string;
  providerId: string;
  modelId: string;
  fileScope: string;
  blockerReason: string | null;
  lastError: string | null;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
}

export interface TaskRef {
  id: number;
  title: string;
  status: TaskStatus;
}

export interface Handoff {
  id: number;
  taskId: number;
  kind: "handoff" | "completion";
  summary: string;
  artifacts: string[];
  nextInstructions: string | null;
  openQuestions: string[];
  createdAt: string;
  reviewedAt: string | null;
  reviewDecision: "accepted" | "returned" | null;
  reviewNote: string | null;
}

export interface UserDecision {
  id: number;
  taskId: number;
  question: string;
  context: string | null;
  options: string[];
  status: "open" | "answered" | "cancelled";
  answer: string | null;
  createdAt: string;
  answeredAt: string | null;
}

export interface ActivityEntry {
  id: number;
  taskId: number;
  kind: string;
  detail: string | null;
  createdAt: string;
}

export interface TaskDetail {
  task: Task;
  dependsOn: TaskRef[];
  dependents: TaskRef[];
  handoffs: Handoff[];
  decisions: UserDecision[];
  activity: ActivityEntry[];
  scopeConflicts: TaskRef[];
}

interface DispatchOutcome {
  taskId: number;
  ok: boolean;
  error: string | null;
}

interface BridgeInfo {
  ready: boolean;
  url: string | null;
  message: string | null;
}

interface ProjectOption {
  id: number;
  name: string;
  archivedAt: string | null;
  isGitRepository: boolean;
}

interface WorkspaceOption {
  id: number;
  name: string;
  projectIds: number[];
}

interface AgentOption {
  id: string;
  name: string;
}

interface ModelOption {
  id: string;
  name: string;
  providerId: string | null;
}

interface TasksPageProps {
  baseUrl: string;
  username: string;
  password: string;
  connected: boolean;
  projects: ProjectOption[];
  workspaces: WorkspaceOption[];
  agents: AgentOption[];
  models: ModelOption[];
}

interface TaskFormState {
  title: string;
  objective: string;
  fileScope: string;
  agentId: string;
  modelIndex: string;
  dependsOnIds: number[];
}

const STATUS_LABELS: Record<TaskStatus, string> = {
  pending: "PENDIENTE",
  ready: "LISTA",
  working: "EN CURSO",
  blocked: "BLOQUEADA",
  review: "EN REVISIÓN",
  completed: "COMPLETADA",
  failed: "FALLIDA",
};

const STATUS_ORDER: TaskStatus[] = [
  "ready",
  "working",
  "blocked",
  "review",
  "pending",
  "completed",
  "failed",
];

const ACTIVITY_LABELS: Record<string, string> = {
  created: "Tarea creada",
  updated: "Definición actualizada",
  working: "Trabajo iniciado",
  dispatched: "Prompt enviado al agente",
  dispatch_failed: "Falló el envío del prompt",
  handoff_submitted: "Entrega registrada por el agente",
  handoff_accepted: "Entrega aceptada",
  handoff_returned: "Entrega devuelta",
  blocked: "Bloqueo reportado",
  decision_requested: "Pregunta al usuario",
  decision_answered: "Respuesta registrada",
  completed_manually: "Cierre manual",
  failed: "Marcada como fallida",
  reopened: "Tarea reabierta",
};

function formatTime(value: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("es", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function latestPendingHandoff(detail: TaskDetail): Handoff | undefined {
  return detail.handoffs.find((handoff) => handoff.reviewedAt === null);
}

function openDecision(detail: TaskDetail): UserDecision | undefined {
  return detail.decisions.find((decision) => decision.status === "open");
}

function latestAcceptedHandoff(detail: TaskDetail): Handoff | undefined {
  return detail.handoffs.find((handoff) => handoff.reviewDecision === "accepted");
}

function latestReturnedHandoff(detail: TaskDetail): Handoff | undefined {
  return detail.handoffs.find((handoff) => handoff.reviewDecision === "returned" && handoff.reviewNote);
}

export default function TasksPage({
  baseUrl,
  username,
  password,
  connected,
  projects,
  workspaces,
  agents,
  models,
}: TasksPageProps) {
  const activeProjects = useMemo(
    () => projects.filter((project) => project.archivedAt === null),
    [projects],
  );
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<number | null>(null);
  const visibleProjects = useMemo(() => {
    if (selectedWorkspaceId === null) return activeProjects;
    const workspace = workspaces.find((entry) => entry.id === selectedWorkspaceId);
    if (!workspace) return activeProjects;
    return activeProjects.filter((project) => workspace.projectIds.includes(project.id));
  }, [activeProjects, workspaces, selectedWorkspaceId]);
  const selectedWorkspace = workspaces.find((entry) => entry.id === selectedWorkspaceId) ?? null;
  const [selectedProjectId, setSelectedProjectId] = useState<number | null>(null);
  const [tasks, setTasks] = useState<TaskDetail[] | null>(null);
  const [tasksError, setTasksError] = useState<string | null>(null);
  const [tasksLoading, setTasksLoading] = useState(false);
  const [bridge, setBridge] = useState<BridgeInfo | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingTask, setEditingTask] = useState<Task | null>(null);
  const [form, setForm] = useState<TaskFormState>(emptyForm());
  const [formError, setFormError] = useState<string | null>(null);
  const [formSaving, setFormSaving] = useState(false);
  const [decisionDrafts, setDecisionDrafts] = useState<Record<number, string>>({});
  const [returnDrafts, setReturnDrafts] = useState<Record<number, string>>({});
  const [closureDrafts, setClosureDrafts] = useState<Record<number, string>>({});

  const projectId = selectedProjectId !== null && visibleProjects.some((project) => project.id === selectedProjectId)
    ? selectedProjectId
    : visibleProjects[0]?.id ?? null;
  const selectedProject = visibleProjects.find((project) => project.id === projectId) ?? null;

  function emptyForm(): TaskFormState {
    return {
      title: "",
      objective: "",
      fileScope: "",
      agentId: agents[0]?.id ?? "",
      modelIndex: "0",
      dependsOnIds: [],
    };
  }

  // React reutiliza los eventos y limpia `event.currentTarget` al terminar el
  // manejador: el valor se lee de forma síncrona y el estado se actualiza con un
  // objeto plano para no tocar el evento cuando React vuelve a ejecutar el updater.
  function patchForm(patch: Partial<TaskFormState>) {
    setForm((current) => ({ ...current, ...patch }));
  }

  const refreshBridge = useCallback(async () => {
    try {
      setBridge(await invoke<BridgeInfo>("bridge_status"));
    } catch {
      setBridge({ ready: false, url: null, message: null });
    }
  }, []);

  const refreshTasks = useCallback(async (project: number) => {
    setTasksLoading(true);
    setTasksError(null);
    try {
      const result = await invoke<TaskDetail[]>("list_project_tasks", { projectId: project });
      setTasks(result);
    } catch (error) {
      setTasksError(String(error));
      setTasks([]);
    } finally {
      setTasksLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshBridge();
  }, [refreshBridge]);

  useEffect(() => {
    if (projectId === null) {
      setTasks(null);
      return;
    }
    void refreshTasks(projectId);
  }, [projectId, refreshTasks]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listen("coordination-changed", () => {
      if (disposed || projectId === null) return;
      void refreshTasks(projectId);
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [projectId, refreshTasks]);

  const orderedTasks = useMemo(() => {
    if (!tasks) return null;
    return [...tasks].sort((left, right) => {
      const order = STATUS_ORDER.indexOf(left.task.status) - STATUS_ORDER.indexOf(right.task.status);
      return order !== 0 ? order : left.task.id - right.task.id;
    });
  }, [tasks]);

  const counts = useMemo(() => {
    const result: Partial<Record<TaskStatus, number>> = {};
    for (const detail of tasks ?? []) {
      result[detail.task.status] = (result[detail.task.status] ?? 0) + 1;
    }
    return result;
  }, [tasks]);

  const readyCount = counts.ready ?? 0;
  const openRequestCount =
    (counts.blocked ?? 0) + (counts.review ?? 0);

  function openCreateDialog() {
    setEditingTask(null);
    setForm(emptyForm());
    setFormError(null);
    setDialogOpen(true);
  }

  function openEditDialog(task: Task) {
    const modelIndex = models.findIndex(
      (model) => model.providerId === task.providerId && model.id === task.modelId,
    );
    setEditingTask(task);
    setForm({
      title: task.title,
      objective: task.objective,
      fileScope: task.fileScope,
      agentId: task.agentId,
      modelIndex: modelIndex >= 0 ? String(modelIndex) : "0",
      dependsOnIds: [],
    });
    setFormError(null);
    setDialogOpen(true);
  }

  async function saveTask(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (projectId === null) return;
    setFormSaving(true);
    setFormError(null);
    try {
      if (editingTask) {
        await invoke("update_task_definition", {
          taskId: editingTask.id,
          title: form.title,
          objective: form.objective,
          fileScope: form.fileScope,
        });
        setActionNotice(`Tarea #${editingTask.id} actualizada.`);
      } else {
        const model = models[Number(form.modelIndex)];
        if (!model?.providerId || !form.agentId) {
          throw new Error("Selecciona un perfil y un modelo disponibles.");
        }
        const created = await invoke<Task>("create_task", {
          projectId,
          title: form.title,
          objective: form.objective,
          agentId: form.agentId,
          providerId: model.providerId,
          modelId: model.id,
          fileScope: form.fileScope,
          dependsOnIds: form.dependsOnIds,
        });
        setActionNotice(`Tarea #${created.id} añadida al plan.`);
      }
      setDialogOpen(false);
      await refreshTasks(projectId);
    } catch (error) {
      setFormError(String(error));
    } finally {
      setFormSaving(false);
    }
  }

  async function runAction(key: string, action: () => Promise<string | null>) {
    setBusyKey(key);
    setActionError(null);
    setActionNotice(null);
    try {
      const notice = await action();
      if (notice) setActionNotice(notice);
    } catch (error) {
      setActionError(String(error));
    } finally {
      setBusyKey(null);
      if (projectId !== null) {
        await refreshTasks(projectId);
      }
    }
  }

  function startTask(task: Task, allowScopeConflicts: boolean) {
    return runAction(`start-${task.id}`, async () => {
      await invoke("start_task", {
        baseUrl,
        username,
        password,
        taskId: task.id,
        allowScopeConflicts,
      });
      return `Tarea #${task.id} enviada a su agente.`;
    });
  }

  function startReadyTasks() {
    if (projectId === null) return;
    return runAction(`start-ready-${projectId}`, async () => {
      const outcomes = await invoke<DispatchOutcome[]>("start_ready_project_tasks", {
        baseUrl,
        username,
        password,
        projectId,
      });
      if (outcomes.length === 0) {
        return "No hay tareas listas para lanzar.";
      }
      const failed = outcomes.filter((outcome) => !outcome.ok);
      if (failed.length === 0) {
        return `${outcomes.length} ${outcomes.length === 1 ? "tarea lanzada" : "tareas lanzadas"}.`;
      }
      throw new Error(
        failed
          .map((outcome) => `#${outcome.taskId}: ${outcome.error ?? "no se pudo lanzar"}`)
          .join(" · "),
      );
    });
  }

  function startReadyWorkspaceTasks() {
    if (selectedWorkspaceId === null) return;
    if (!window.confirm(`¿Lanzar las tareas listas de todos los proyectos del espacio «${selectedWorkspace?.name}»? Puede arrancar varios agentes a la vez.`)) return;
    return runAction(`start-ready-workspace-${selectedWorkspaceId}`, async () => {
      const outcomes = await invoke<DispatchOutcome[]>("start_ready_workspace_tasks", {
        baseUrl,
        username,
        password,
        workspaceId: selectedWorkspaceId,
      });
      if (outcomes.length === 0) {
        return "No hay tareas listas para lanzar en este espacio.";
      }
      const failed = outcomes.filter((outcome) => !outcome.ok);
      if (failed.length === 0) {
        return `${outcomes.length} ${outcomes.length === 1 ? "tarea lanzada" : "tareas lanzadas"} en el espacio.`;
      }
      throw new Error(
        failed
          .map((outcome) => `#${outcome.taskId}: ${outcome.error ?? "no se pudo lanzar"}`)
          .join(" · "),
      );
    });
  }

  function acceptHandoff(handoff: Handoff, andContinue: boolean) {
    return runAction(`accept-${handoff.id}`, async () => {
      await invoke("accept_task_handoff", { handoffId: handoff.id });
      if (andContinue && projectId !== null) {
        const outcomes = await invoke<DispatchOutcome[]>("start_ready_project_tasks", {
          baseUrl,
          username,
          password,
          projectId,
        });
        const launched = outcomes.filter((outcome) => outcome.ok);
        return launched.length > 0
          ? `Entrega aceptada y ${launched.length} ${launched.length === 1 ? "tarea dependiente lanzada" : "tareas dependientes lanzadas"}.`
          : "Entrega aceptada. No quedan tareas listas por lanzar.";
      }
      return "Entrega aceptada.";
    });
  }

  function returnHandoff(handoff: Handoff) {
    const note = (returnDrafts[handoff.id] ?? "").trim();
    return runAction(`return-${handoff.id}`, async () => {
      await invoke("return_task_handoff", {
        baseUrl,
        username,
        password,
        handoffId: handoff.id,
        note,
      });
      setReturnDrafts((current) => ({ ...current, [handoff.id]: "" }));
      return "Entrega devuelta al agente con tu nota.";
    });
  }

  function answerDecision(decision: UserDecision) {
    const answer = (decisionDrafts[decision.id] ?? "").trim();
    return runAction(`answer-${decision.id}`, async () => {
      await invoke("answer_task_decision", {
        baseUrl,
        username,
        password,
        decisionId: decision.id,
        answer,
      });
      setDecisionDrafts((current) => ({ ...current, [decision.id]: "" }));
      return "Respuesta enviada al agente.";
    });
  }

  function completeManually(task: Task) {
    const summary = (closureDrafts[task.id] ?? "").trim();
    return runAction(`complete-${task.id}`, async () => {
      await invoke("complete_task_manually", { taskId: task.id, summary });
      setClosureDrafts((current) => ({ ...current, [task.id]: "" }));
      return `Tarea #${task.id} cerrada.`;
    });
  }

  function failTask(task: Task) {
    const note = (closureDrafts[task.id] ?? "").trim();
    return runAction(`fail-${task.id}`, async () => {
      await invoke("fail_task", { taskId: task.id, note });
      setClosureDrafts((current) => ({ ...current, [task.id]: "" }));
      return `Tarea #${task.id} marcada como fallida.`;
    });
  }

  function reopenTask(task: Task) {
    return runAction(`reopen-${task.id}`, async () => {
      await invoke("reopen_task", { taskId: task.id });
      return `Tarea #${task.id} reabierta.`;
    });
  }

  function refreshBridgeConfig() {
    if (projectId === null) return;
    return runAction(`bridge-${projectId}`, async () => {
      await invoke("refresh_project_bridge_config", { projectId });
      await refreshBridge();
      return "Configuración del puente reescrita para este proyecto.";
    });
  }

  const canDispatch = connected && bridge?.ready === true;

  return (
    <section className="tasks-page">
      <div className="page-heading tasks-page-heading">
        <div>
          <p className="eyebrow">COORDINACIÓN DURABLE</p>
          <h1>Tareas</h1>
          <p className="page-description">
            Planifica tareas dependientes, lánzalas en su propio worktree y revisa cada entrega antes de
            desbloquear al siguiente agente.
          </p>
        </div>
        <div className="tasks-heading-actions">
          <button className="primary-button" type="button" onClick={openCreateDialog} disabled={projectId === null || !connected}>
            <span aria-hidden="true">＋</span> Nueva tarea
          </button>
        </div>
      </div>

      <div className="tasks-toolbar panel">
        <div className="tasks-toolbar-row">
          {workspaces.length > 0 && (
            <label className="tasks-project-select">
              <span>Espacio</span>
              <select
                value={selectedWorkspaceId ?? ""}
                onChange={(event) => {
                  const value = event.currentTarget.value;
                  setSelectedWorkspaceId(value === "" ? null : Number(value));
                  setSelectedProjectId(null);
                }}
              >
                <option value="">Todos los proyectos</option>
                {workspaces.map((workspace) => (
                  <option key={workspace.id} value={workspace.id}>{workspace.name}</option>
                ))}
              </select>
            </label>
          )}
          <label className="tasks-project-select">
            <span>Proyecto{selectedWorkspace ? ` · ${selectedWorkspace.name}` : ""}</span>
            <select
              value={projectId ?? ""}
              onChange={(event) => {
                const value = event.currentTarget.value;
                setSelectedProjectId(value === "" ? null : Number(value));
              }}
              disabled={visibleProjects.length === 0}
            >
              {visibleProjects.length === 0 && <option value="">Sin proyectos en este espacio</option>}
              {visibleProjects.map((project) => (
                <option key={project.id} value={project.id}>{project.name}</option>
              ))}
            </select>
          </label>
          <div className="tasks-toolbar-buttons">
            <button
              className="primary-button"
              type="button"
              onClick={() => void startReadyTasks()}
              disabled={!canDispatch || readyCount === 0 || busyKey === `start-ready-${projectId}`}
            >
              {busyKey === `start-ready-${projectId}` ? "Lanzando…" : `Lanzar tareas listas${readyCount ? ` (${readyCount})` : ""}`}
            </button>
            {selectedWorkspaceId !== null && (
              <button
                className="secondary-button"
                type="button"
                onClick={() => void startReadyWorkspaceTasks()}
                disabled={!canDispatch || busyKey === `start-ready-workspace-${selectedWorkspaceId}`}
                title="Lanza las tareas listas de todos los proyectos del espacio"
              >
                {busyKey === `start-ready-workspace-${selectedWorkspaceId}` ? "Lanzando espacio…" : "Lanzar espacio"}
              </button>
            )}
            <button
              className="secondary-button"
              type="button"
              onClick={() => projectId !== null && void refreshTasks(projectId)}
              disabled={projectId === null || tasksLoading}
            >
              {tasksLoading ? "Actualizando…" : "Actualizar"}
            </button>
          </div>
        </div>
        <div className="tasks-bridge-row">
          <span className={`tasks-bridge-badge ${bridge?.ready ? "bridge-ready" : "bridge-missing"}`}>
            {bridge?.ready ? "PUENTE MCP ACTIVO" : "PUENTE MCP NO DISPONIBLE"}
          </span>
          {bridge?.url && <code>{bridge.url}</code>}
          <span className="tasks-bridge-note">
            Las herramientas de coordinación viajan por loopback con un token por ejecución; la
            configuración se escribe junto a los worktrees del proyecto, nunca en tu repositorio.
          </span>
          <button
            className="secondary-button"
            type="button"
            onClick={() => void refreshBridgeConfig()}
            disabled={projectId === null || busyKey === `bridge-${projectId}`}
          >
            {busyKey === `bridge-${projectId}` ? "Escribiendo…" : "Regenerar configuración"}
          </button>
        </div>
        {!connected && <p className="tasks-inline-warning">Conecta OpenCode para lanzar tareas y responder a los agentes.</p>}
        {!bridge?.ready && bridge?.message && <p className="tasks-inline-warning">{bridge.message}</p>}
        {selectedProject && !selectedProject.isGitRepository && (
          <p className="tasks-inline-warning">
            Este proyecto no es un repositorio Git; para lanzar una tarea se necesita un worktree aislado.
          </p>
        )}
      </div>

      {actionNotice && <div className="feedback success-feedback tasks-feedback" role="status"><span className="feedback-icon">✓</span><div><span>{actionNotice}</span></div></div>}
      {actionError && <div className="feedback error-feedback tasks-feedback" role="alert"><span className="feedback-icon">!</span><div><span>{actionError}</span></div></div>}
      {tasksError && <div className="feedback error-feedback tasks-feedback" role="alert"><span className="feedback-icon">!</span><div><div><strong>No se pudieron cargar las tareas</strong><span>{tasksError}</span></div></div></div>}

      {projectId === null ? (
        <div className="project-empty-state tasks-empty">
          <span className="empty-folder-icon" aria-hidden="true">▱</span>
          <h2>Sin proyectos activos</h2>
          <p>Registra un proyecto con repositorio Git para planificar tareas.</p>
        </div>
      ) : tasksLoading && tasks === null ? (
        <div className="project-empty-state tasks-empty">Cargando el plan…</div>
      ) : (orderedTasks ?? []).length === 0 ? (
        <div className="project-empty-state tasks-empty">
          <span className="empty-folder-icon" aria-hidden="true">▱</span>
          <h2>El plan está vacío</h2>
          <p>Crea la primera tarea, asígnale un perfil y un modelo, y añade dependencias si las necesita.</p>
          <button className="secondary-button" type="button" onClick={openCreateDialog} disabled={!connected}>
            Crear primera tarea
          </button>
        </div>
      ) : (
        <div className="task-list">
          {(orderedTasks ?? []).map((detail) => (
            <TaskCard
              key={detail.task.id}
              detail={detail}
              connected={connected}
              canDispatch={canDispatch}
              busyKey={busyKey}
              decisionDrafts={decisionDrafts}
              returnDrafts={returnDrafts}
              closureDrafts={closureDrafts}
              onDecisionDraft={(id, value) => setDecisionDrafts((current) => ({ ...current, [id]: value }))}
              onReturnDraft={(id, value) => setReturnDrafts((current) => ({ ...current, [id]: value }))}
              onClosureDraft={(id, value) => setClosureDrafts((current) => ({ ...current, [id]: value }))}
              onStart={startTask}
              onAccept={acceptHandoff}
              onReturn={returnHandoff}
              onAnswer={answerDecision}
              onComplete={completeManually}
              onFail={failTask}
              onReopen={reopenTask}
              onEdit={openEditDialog}
            />
          ))}
        </div>
      )}

      {openRequestCount > 0 && (
        <p className="tasks-footer-note">
          {openRequestCount} {openRequestCount === 1 ? "tarea espera" : "tareas esperan"} tu revisión o respuesta.
        </p>
      )}

      {dialogOpen && (
        <div className="project-modal-backdrop" role="presentation" onClick={() => !formSaving && setDialogOpen(false)}>
          <div className="project-modal task-modal" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <div className="project-modal-header">
              <div>
                <p className="eyebrow">{editingTask ? "EDITAR TAREA" : "NUEVA TAREA"}</p>
                <h2>{editingTask ? `Tarea #${editingTask.id}` : "Añadir al plan"}</h2>
              </div>
              <button className="modal-close-button" type="button" onClick={() => setDialogOpen(false)} disabled={formSaving}>✕</button>
            </div>
            <form className="project-form" onSubmit={saveTask}>
              <label htmlFor="task-title">Título</label>
              <input
                id="task-title"
                value={form.title}
                onChange={(event) => patchForm({ title: event.currentTarget.value })}
                placeholder="Implementar el endpoint de sesiones"
                maxLength={120}
                required
              />
              <label htmlFor="task-objective">Objetivo <span>qué debe conseguir el agente</span></label>
              <textarea
                id="task-objective"
                value={form.objective}
                onChange={(event) => patchForm({ objective: event.currentTarget.value })}
                placeholder="Exponer el contrato aprobado, con pruebas de error de conexión."
                rows={4}
                maxLength={8000}
                required
              />
              <label htmlFor="task-scope">Ámbito de archivos <span>opcional, una ruta o patrón por línea</span></label>
              <textarea
                id="task-scope"
                value={form.fileScope}
                onChange={(event) => patchForm({ fileScope: event.currentTarget.value })}
                placeholder={"apps/desktop/src-tauri/src/opencode.rs\napps/desktop/src-tauri/src/opencode/*.rs"}
                rows={3}
                maxLength={4000}
              />
              {!editingTask && (
                <>
                  <label htmlFor="task-agent">Perfil y modelo</label>
                  <div className="task-form-row">
                    <select
                      id="task-agent"
                      value={form.agentId}
                      onChange={(event) => patchForm({ agentId: event.currentTarget.value })}
                      required
                    >
                      {agents.length === 0 && <option value="">Sin perfiles disponibles</option>}
                      {agents.map((agent) => (
                        <option key={agent.id} value={agent.id}>{agent.name}</option>
                      ))}
                    </select>
                    <select
                      value={form.modelIndex}
                      onChange={(event) => patchForm({ modelIndex: event.currentTarget.value })}
                      required
                    >
                      {models.length === 0 && <option value="0">Sin modelos disponibles</option>}
                      {models.map((model, index) => (
                        <option key={`${model.providerId}/${model.id}`} value={index}>
                          {model.name} · {model.providerId}
                        </option>
                      ))}
                    </select>
                  </div>
                  <label>Dependencias <span>la tarea espera a que se acepten sus entregas</span></label>
                  <div className="task-dependency-picker">
                    {(tasks ?? []).length === 0 && <p className="task-dependency-empty">Aún no hay otras tareas en este proyecto.</p>}
                    {(tasks ?? []).map((detail) => (
                      <label key={detail.task.id} className="task-dependency-option">
                        <input
                          type="checkbox"
                          checked={form.dependsOnIds.includes(detail.task.id)}
                          onChange={(event) => {
                            const checked = event.currentTarget.checked;
                            setForm((current) => ({
                              ...current,
                              dependsOnIds: checked
                                ? [...current.dependsOnIds, detail.task.id]
                                : current.dependsOnIds.filter((id) => id !== detail.task.id),
                            }));
                          }}
                        />
                        <span>#{detail.task.id} · {detail.task.title}</span>
                        <span className={`task-status status-${detail.task.status}`}>{STATUS_LABELS[detail.task.status]}</span>
                      </label>
                    ))}
                  </div>
                </>
              )}
              <p className="project-form-hint">
                La app no guarda el texto del prompt: solo el objetivo, la entrega y las decisiones, para que
                el siguiente agente reciba el contexto.
              </p>
              {formError && <p className="project-form-error">{formError}</p>}
              <div className="project-form-actions">
                <button className="secondary-button" type="button" onClick={() => setDialogOpen(false)} disabled={formSaving}>
                  Cancelar
                </button>
                <button className="primary-button" type="submit" disabled={formSaving || agents.length === 0 || models.length === 0}>
                  {formSaving ? "Guardando…" : editingTask ? "Guardar cambios" : "Añadir tarea"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </section>
  );
}

interface TaskCardProps {
  detail: TaskDetail;
  connected: boolean;
  canDispatch: boolean;
  busyKey: string | null;
  decisionDrafts: Record<number, string>;
  returnDrafts: Record<number, string>;
  closureDrafts: Record<number, string>;
  onDecisionDraft: (id: number, value: string) => void;
  onReturnDraft: (id: number, value: string) => void;
  onClosureDraft: (id: number, value: string) => void;
  onStart: (task: Task, allowScopeConflicts: boolean) => Promise<void>;
  onAccept: (handoff: Handoff, andContinue: boolean) => Promise<void>;
  onReturn: (handoff: Handoff) => Promise<void>;
  onAnswer: (decision: UserDecision) => Promise<void>;
  onComplete: (task: Task) => Promise<void>;
  onFail: (task: Task) => Promise<void>;
  onReopen: (task: Task) => Promise<void>;
  onEdit: (task: Task) => void;
}

function TaskCard({
  detail,
  connected,
  canDispatch,
  busyKey,
  decisionDrafts,
  returnDrafts,
  closureDrafts,
  onDecisionDraft,
  onReturnDraft,
  onClosureDraft,
  onStart,
  onAccept,
  onReturn,
  onAnswer,
  onComplete,
  onFail,
  onReopen,
  onEdit,
}: TaskCardProps) {
  const { task } = detail;
  const pendingHandoff = latestPendingHandoff(detail);
  const acceptedHandoff = latestAcceptedHandoff(detail);
  const returnedHandoff = latestReturnedHandoff(detail);
  const outstandingDecision = openDecision(detail);
  const closuresEditable = ["working", "blocked", "review", "failed"].includes(task.status);
  const closureDraft = closureDrafts[task.id] ?? "";

  function requestStart() {
    if (detail.scopeConflicts.length === 0) {
      void onStart(task, false);
      return;
    }
    const titles = detail.scopeConflicts.map((conflict) => `#${conflict.id} «${conflict.title}»`).join(", ");
    if (window.confirm(`El ámbito de archivos se solapa con ${titles}. ¿Lanzar de todos modos?`)) {
      void onStart(task, true);
    }
  }

  return (
    <article className={`task-card task-${task.status}`}>
      <div className="task-card-heading">
        <div className="task-card-title">
          <span className="task-id">#{task.id}</span>
          <h2>{task.title}</h2>
          <span className={`task-status status-${task.status}`}>{STATUS_LABELS[task.status]}</span>
          {task.startedAt && <span className="task-timestamp">Inicio {formatTime(task.startedAt)}</span>}
          {task.completedAt && <span className="task-timestamp">Fin {formatTime(task.completedAt)}</span>}
        </div>
        <div className="task-card-actions">
          {task.status === "ready" && (
            <button
              className="primary-button"
              type="button"
              onClick={requestStart}
              disabled={!canDispatch || busyKey === `start-${task.id}`}
            >
              {busyKey === `start-${task.id}` ? "Lanzando…" : "Lanzar"}
            </button>
          )}
          {task.status === "pending" && (
            <span className="task-waiting-note">Espera entregas aceptadas de sus dependencias</span>
          )}
          {["pending", "ready", "failed"].includes(task.status) && (
            <button className="project-action-button" type="button" onClick={() => onEdit(task)}>Editar</button>
          )}
          {task.status === "completed" && (
            <button className="project-action-button" type="button" onClick={() => void onReopen(task)} disabled={busyKey === `reopen-${task.id}`}>
              {busyKey === `reopen-${task.id}` ? "Reabriendo…" : "Reabrir"}
            </button>
          )}
        </div>
      </div>

      <p className="task-objective">{task.objective}</p>

      <div className="task-meta-row">
        <span className="task-meta-chip">{task.agentId} · {task.providerId}/{task.modelId}</span>
        {task.worktreeId && <span className="task-meta-chip">Worktree #{task.worktreeId}</span>}
        {task.fileScope.trim() !== "" && <span className="task-meta-chip scope-chip" title={task.fileScope}>Ámbito: {task.fileScope.split("\n")[0]}{task.fileScope.split("\n").length > 1 ? "…" : ""}</span>}
      </div>

      {detail.dependsOn.length > 0 && (
        <p className="task-links">
          Depende de:{" "}
          {detail.dependsOn.map((dependency, index) => (
            <span key={dependency.id}>
              {index > 0 && ", "}
              #{dependency.id} «{dependency.title}» {dependency.status === "completed" ? "✓" : "…"}
            </span>
          ))}
        </p>
      )}
      {detail.dependents.length > 0 && (
        <p className="task-links subtle">
          Desbloquea:{" "}
          {detail.dependents.map((dependent, index) => (
            <span key={dependent.id}>
              {index > 0 && ", "}
              #{dependent.id} «{dependent.title}»
            </span>
          ))}
        </p>
      )}

      {detail.scopeConflicts.length > 0 && (
        <p className="task-conflict-warning" role="alert">
          El ámbito de archivos se solapa con: {detail.scopeConflicts.map((conflict) => `#${conflict.id} «${conflict.title}»`).join(", ")}.
          Lanzar de todos modos puede causar conflictos de edición.
        </p>
      )}
      {task.blockerReason && <p className="task-blocker">Bloqueo: {task.blockerReason}</p>}
      {task.lastError && <p className="task-inline-error">Último error: {task.lastError}</p>}

      {outstandingDecision && (
        <div className="task-decision-panel">
          <div className="task-decision-heading">
            <strong>El agente necesita una decisión</strong>
            <span className="task-status status-blocked">BLOQUEADA</span>
          </div>
          <p className="task-decision-question">{outstandingDecision.question}</p>
          {outstandingDecision.context && <p className="task-decision-context">{outstandingDecision.context}</p>}
          {outstandingDecision.options.length > 0 && (
            <div className="task-decision-options">
              {outstandingDecision.options.map((option) => (
                <button
                  key={option}
                  className="secondary-button"
                  type="button"
                  onClick={() => onDecisionDraft(outstandingDecision.id, option)}
                >
                  {option}
                </button>
              ))}
            </div>
          )}
          <textarea
            value={decisionDrafts[outstandingDecision.id] ?? ""}
            onChange={(event) => onDecisionDraft(outstandingDecision.id, event.currentTarget.value)}
            placeholder="Escribe la decisión que debe seguir el agente"
            rows={2}
          />
          <div className="task-decision-actions">
            <button
              className="primary-button"
              type="button"
              onClick={() => void onAnswer(outstandingDecision)}
              disabled={!connected || (decisionDrafts[outstandingDecision.id] ?? "").trim() === "" || busyKey === `answer-${outstandingDecision.id}`}
            >
              {busyKey === `answer-${outstandingDecision.id}` ? "Enviando…" : "Responder y continuar"}
            </button>
            <span>La respuesta se envía a la sesión del agente y queda en el historial de la tarea.</span>
          </div>
        </div>
      )}

      {pendingHandoff && (
        <div className="task-handoff-panel">
          <div className="task-handoff-heading">
            <strong>{pendingHandoff.kind === "completion" ? "Cierre propuesto por el agente" : "Entrega pendiente de revisión"}</strong>
            <span className="worktree-status status-waiting">REVISAR</span>
          </div>
          <p className="task-handoff-summary">{pendingHandoff.summary}</p>
          {pendingHandoff.artifacts.length > 0 && (
            <div className="task-handoff-artifacts">
              {pendingHandoff.artifacts.map((artifact) => <code key={artifact}>{artifact}</code>)}
            </div>
          )}
          {pendingHandoff.nextInstructions && (
            <p className="task-handoff-instructions">Instrucciones para el siguiente rol: {pendingHandoff.nextInstructions}</p>
          )}
          {pendingHandoff.openQuestions.length > 0 && (
            <ul className="task-handoff-questions">
              {pendingHandoff.openQuestions.map((question) => <li key={question}>{question}</li>)}
            </ul>
          )}
          <textarea
            value={returnDrafts[pendingHandoff.id] ?? ""}
            onChange={(event) => onReturnDraft(pendingHandoff.id, event.currentTarget.value)}
            placeholder="Nota de devolución si el trabajo no está listo"
            rows={2}
          />
          <div className="task-handoff-actions">
            <button
              className="primary-button"
              type="button"
              onClick={() => void onAccept(pendingHandoff, true)}
              disabled={busyKey === `accept-${pendingHandoff.id}`}
            >
              {busyKey === `accept-${pendingHandoff.id}` ? "Aceptando…" : "Aceptar y lanzar listas"}
            </button>
            <button
              className="secondary-button"
              type="button"
              onClick={() => void onAccept(pendingHandoff, false)}
              disabled={busyKey === `accept-${pendingHandoff.id}`}
            >
              Solo aceptar
            </button>
            <button
              className="stop-server-button"
              type="button"
              onClick={() => void onReturn(pendingHandoff)}
              disabled={!connected || (returnDrafts[pendingHandoff.id] ?? "").trim() === "" || busyKey === `return-${pendingHandoff.id}`}
            >
              {busyKey === `return-${pendingHandoff.id}` ? "Devolviendo…" : "Devolver con nota"}
            </button>
          </div>
        </div>
      )}

      {!pendingHandoff && acceptedHandoff && task.status === "completed" && (
        <details className="task-history">
          <summary>Entrega aceptada · {formatTime(acceptedHandoff.reviewedAt)}</summary>
          <p>{acceptedHandoff.summary}</p>
          {acceptedHandoff.artifacts.length > 0 && (
            <div className="task-handoff-artifacts">
              {acceptedHandoff.artifacts.map((artifact) => <code key={artifact}>{artifact}</code>)}
            </div>
          )}
        </details>
      )}

      {returnedHandoff && (
        <p className="task-links subtle">Última devolución: {returnedHandoff.reviewNote}</p>
      )}

      {closuresEditable && (
        <div className="task-closure-row">
          <input
            value={closureDraft}
            onChange={(event) => onClosureDraft(task.id, event.currentTarget.value)}
            placeholder="Nota de cierre o motivo de fallo"
            maxLength={8000}
          />
          <button
            className="secondary-button"
            type="button"
            onClick={() => void onComplete(task)}
            disabled={closureDraft.trim() === "" || busyKey === `complete-${task.id}`}
          >
            {busyKey === `complete-${task.id}` ? "Cerrando…" : "Cerrar manualmente"}
          </button>
          {task.status !== "failed" && (
            <button
              className="project-action-button"
              type="button"
              onClick={() => void onFail(task)}
              disabled={closureDraft.trim() === "" || busyKey === `fail-${task.id}`}
            >
              {busyKey === `fail-${task.id}` ? "Guardando…" : "Marcar fallida"}
            </button>
          )}
        </div>
      )}

      <details className="task-history">
        <summary>Actividad ({detail.activity.length})</summary>
        <ul>
          {detail.activity.slice(0, 12).map((entry) => (
            <li key={entry.id}>
              <span>{ACTIVITY_LABELS[entry.kind] ?? entry.kind}</span>
              {entry.detail && <em> · {entry.detail}</em>}
              <time>{formatTime(entry.createdAt)}</time>
            </li>
          ))}
          {detail.activity.length === 0 && <li>Sin movimientos todavía.</li>}
        </ul>
      </details>
    </article>
  );
}
