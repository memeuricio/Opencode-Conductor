import { useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDirectoryDialog } from "@tauri-apps/plugin-dialog";
import brandLogo from "./assets/logo.webp";
import "./App.css";
import "./brand.css";

interface OpenCodeHealth {
  healthy: boolean;
  version: string;
}

interface StartedOpenCodeServer {
  baseUrl: string;
  username: string;
  password: string;
  version: string;
}

interface ManagedOpenCodeServerStatus {
  running: boolean;
  baseUrl: string | null;
  version: string | null;
}

interface AgentSummary {
  id: string;
  name: string;
  mode: string | null;
  description: string | null;
  hidden: boolean | null;
}

interface ModelSummary {
  id: string;
  modelId: string | null;
  providerId: string | null;
  name: string;
  enabled: boolean | null;
}

interface OpenCodeCatalog {
  agents: AgentSummary[] | null;
  models: ModelSummary[] | null;
  warnings: string[];
}

interface Project {
  id: number;
  name: string;
  description: string | null;
  rootPath: string;
  isGitRepository: boolean;
  createdAt: string;
  updatedAt: string;
  archivedAt: string | null;
}

interface ProjectForm {
  name: string;
  rootPath: string;
  description: string;
}

interface Worktree {
  id: number;
  projectId: number;
  label: string;
  branchName: string;
  directory: string;
  baseCommit: string;
  status: "creating" | "ready" | "failed";
  lastError: string | null;
  createdAt: string;
  opencodeSessionId: string | null;
  opencodeAgentId: string | null;
  opencodeProviderId: string | null;
  opencodeModelId: string | null;
  opencodeLocationDirectory: string | null;
  opencodeLocationMatches: boolean | null;
}

interface PendingPermission {
  id: string;
  action: string;
  resources: string[];
  message: string | null;
}

interface SessionSnapshot {
  active: boolean;
  latestResponse: string | null;
  responseCompleted: boolean;
  permissions: PendingPermission[];
  permissionWarning: string | null;
}

type ConnectionState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "connected"; version: string }
  | { kind: "error"; message: string };

type CatalogState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "loaded"; catalog: OpenCodeCatalog }
  | { kind: "error"; message: string };

function App() {
  const [page, setPage] = useState<"dashboard" | "projects">("dashboard");
  const [baseUrl, setBaseUrl] = useState("http://127.0.0.1:4096");
  const [username, setUsername] = useState("opencode");
  const [password, setPassword] = useState("");
  const [managedServerState, setManagedServerState] = useState<"stopped" | "starting" | "running" | "stopping">("stopped");
  const [connection, setConnection] = useState<ConnectionState>({ kind: "idle" });
  const [catalogState, setCatalogState] = useState<CatalogState>({ kind: "idle" });
  const [projects, setProjects] = useState<Project[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(true);
  const [projectsError, setProjectsError] = useState<string | null>(null);
  const [showArchivedProjects, setShowArchivedProjects] = useState(false);
  const [projectDialogOpen, setProjectDialogOpen] = useState(false);
  const [editingProject, setEditingProject] = useState<Project | null>(null);
  const [projectForm, setProjectForm] = useState<ProjectForm>({ name: "", rootPath: "", description: "" });
  const [projectFormError, setProjectFormError] = useState<string | null>(null);
  const [projectSaving, setProjectSaving] = useState(false);
  const [projectBusyId, setProjectBusyId] = useState<number | null>(null);
  const [expandedProjectId, setExpandedProjectId] = useState<number | null>(null);
  const [worktreesByProject, setWorktreesByProject] = useState<Record<number, Worktree[]>>({});
  const [worktreesLoadingProjectId, setWorktreesLoadingProjectId] = useState<number | null>(null);
  const [worktreesErrors, setWorktreesErrors] = useState<Record<number, string>>({});
  const [worktreeDialogProject, setWorktreeDialogProject] = useState<Project | null>(null);
  const [worktreeLabel, setWorktreeLabel] = useState("");
  const [worktreeFormError, setWorktreeFormError] = useState<string | null>(null);
  const [worktreeSaving, setWorktreeSaving] = useState(false);
  const [sessionDialogWorktree, setSessionDialogWorktree] = useState<Worktree | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [selectedModelIndex, setSelectedModelIndex] = useState("0");
  const [sessionFormError, setSessionFormError] = useState<string | null>(null);
  const [sessionSaving, setSessionSaving] = useState(false);
  const [taskDialogWorktree, setTaskDialogWorktree] = useState<Worktree | null>(null);
  const [taskPrompt, setTaskPrompt] = useState("");
  const [taskSending, setTaskSending] = useState(false);
  const [taskFormError, setTaskFormError] = useState<string | null>(null);
  const [taskSnapshots, setTaskSnapshots] = useState<Record<number, SessionSnapshot>>({});
  const [taskSnapshotLoadingId, setTaskSnapshotLoadingId] = useState<number | null>(null);
  const [taskSnapshotErrors, setTaskSnapshotErrors] = useState<Record<number, string>>({});
  const [taskAcceptedAt, setTaskAcceptedAt] = useState<Record<number, string>>({});
  const [permissionBusyId, setPermissionBusyId] = useState<string | null>(null);

  async function refreshProjects(includeArchived: boolean) {
    setProjectsLoading(true);
    setProjectsError(null);
    try {
      const result = await invoke<Project[]>("list_projects", { includeArchived });
      setProjects(result);
    } catch (error) {
      setProjectsError(String(error));
    } finally {
      setProjectsLoading(false);
    }
  }

  useEffect(() => {
    void refreshProjects(false);
  }, []);

  useEffect(() => {
    if (managedServerState !== "running") return;
    let disposed = false;
    const checkManagedServer = async () => {
      try {
        const status = await invoke<ManagedOpenCodeServerStatus>("managed_opencode_server_status");
        if (!disposed && !status.running) {
          setManagedServerState("stopped");
          setPassword("");
          setConnection({ kind: "error", message: "El servidor OpenCode iniciado por Stade Studio se detuvo." });
          setCatalogState({ kind: "idle" });
        }
      } catch {
        // A transient IPC failure should not be mistaken for a crashed server.
      }
    };
    const interval = window.setInterval(() => void checkManagedServer(), 2500);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [managedServerState]);

  function openProjectDialog(project?: Project) {
    setEditingProject(project ?? null);
    setProjectForm(project
      ? { name: project.name, rootPath: project.rootPath, description: project.description ?? "" }
      : { name: "", rootPath: "", description: "" });
    setProjectFormError(null);
    setProjectDialogOpen(true);
  }

  async function chooseProjectDirectory() {
    try {
      const selected = await openDirectoryDialog({
        directory: true,
        multiple: false,
        title: "Selecciona la carpeta raíz del proyecto",
      });
      if (typeof selected === "string") {
        setProjectForm((current) => ({ ...current, rootPath: selected }));
      }
    } catch (error) {
      setProjectFormError(`No se pudo abrir el selector de carpetas: ${String(error)}`);
    }
  }

  async function saveProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setProjectSaving(true);
    setProjectFormError(null);

    try {
      if (editingProject) {
        await invoke<Project>("update_project", {
          projectId: editingProject.id,
          name: projectForm.name,
          description: projectForm.description || null,
        });
      } else {
        await invoke<Project>("create_project", {
          name: projectForm.name,
          rootPath: projectForm.rootPath,
          description: projectForm.description || null,
        });
      }
      setProjectDialogOpen(false);
      await refreshProjects(showArchivedProjects);
    } catch (error) {
      setProjectFormError(String(error));
    } finally {
      setProjectSaving(false);
    }
  }

  async function toggleProjectArchived(project: Project) {
    setProjectBusyId(project.id);
    setProjectsError(null);
    try {
      await invoke<Project>("set_project_archived", {
        projectId: project.id,
        archived: project.archivedAt === null,
      });
      await refreshProjects(showArchivedProjects);
    } catch (error) {
      setProjectsError(String(error));
    } finally {
      setProjectBusyId(null);
    }
  }

  async function refreshWorktrees(projectId: number) {
    setWorktreesLoadingProjectId(projectId);
    setWorktreesErrors((current) => ({ ...current, [projectId]: "" }));
    try {
      const result = await invoke<Worktree[]>("list_project_worktrees", { projectId });
      setWorktreesByProject((current) => ({ ...current, [projectId]: result }));
    } catch (error) {
      setWorktreesErrors((current) => ({ ...current, [projectId]: String(error) }));
    } finally {
      setWorktreesLoadingProjectId(null);
    }
  }

  function toggleProjectEnvironments(project: Project) {
    if (expandedProjectId === project.id) {
      setExpandedProjectId(null);
      return;
    }
    setExpandedProjectId(project.id);
    void refreshWorktrees(project.id);
  }

  function openWorktreeDialog(project: Project) {
    setWorktreeDialogProject(project);
    setWorktreeLabel("");
    setWorktreeFormError(null);
  }

  async function saveWorktree(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!worktreeDialogProject) return;

    setWorktreeSaving(true);
    setWorktreeFormError(null);
    try {
      await invoke<Worktree>("create_project_worktree", {
        projectId: worktreeDialogProject.id,
        label: worktreeLabel,
      });
      const projectId = worktreeDialogProject.id;
      setWorktreeDialogProject(null);
      setExpandedProjectId(projectId);
      await refreshWorktrees(projectId);
    } catch (error) {
      setWorktreeFormError(String(error));
      await refreshWorktrees(worktreeDialogProject.id);
    } finally {
      setWorktreeSaving(false);
    }
  }

  const selectableAgents = catalogState.kind === "loaded"
    ? (catalogState.catalog.agents ?? []).filter((agent) => !agent.hidden && (agent.mode === "primary" || agent.mode === "all"))
    : [];
  const selectableModels = catalogState.kind === "loaded"
    ? (catalogState.catalog.models ?? []).filter((model) => model.enabled === true && model.providerId)
    : [];

  function openSessionDialog(worktree: Worktree) {
    if (connection.kind !== "connected" || catalogState.kind !== "loaded") return;
    setSessionDialogWorktree(worktree);
    setSelectedAgentId(selectableAgents[0]?.id ?? "");
    setSelectedModelIndex("0");
    setSessionFormError(null);
  }

  async function createOpenCodeSession(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!sessionDialogWorktree || connection.kind !== "connected" || catalogState.kind !== "loaded") return;
    const model = selectableModels[Number(selectedModelIndex)];
    if (!model?.providerId || !selectedAgentId) {
      setSessionFormError("Selecciona un perfil primario y un modelo disponible.");
      return;
    }

    setSessionSaving(true);
    setSessionFormError(null);
    try {
      await invoke<Worktree>("create_worktree_session", {
        baseUrl,
        username,
        password,
        worktreeId: sessionDialogWorktree.id,
        agentId: selectedAgentId,
        providerId: model.providerId,
        modelId: model.id,
      });
      const projectId = sessionDialogWorktree.projectId;
      setSessionDialogWorktree(null);
      await refreshWorktrees(projectId);
    } catch (error) {
      setSessionFormError(String(error));
      await refreshWorktrees(sessionDialogWorktree.projectId);
    } finally {
      setSessionSaving(false);
    }
  }

  function openTaskDialog(worktree: Worktree) {
    setTaskDialogWorktree(worktree);
    setTaskPrompt("");
    setTaskFormError(null);
  }

  async function sendTaskPrompt(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!taskDialogWorktree || connection.kind !== "connected") return;

    setTaskSending(true);
    setTaskFormError(null);
    try {
      await invoke("send_worktree_prompt", {
        baseUrl,
        username,
        password,
        worktreeId: taskDialogWorktree.id,
        text: taskPrompt,
      });
      const worktree = taskDialogWorktree;
      setTaskAcceptedAt((current) => ({ ...current, [worktree.id]: new Date().toLocaleTimeString() }));
      setTaskDialogWorktree(null);
      setTaskPrompt("");
      void refreshTaskSnapshot(worktree);
    } catch (error) {
      setTaskFormError(String(error));
    } finally {
      setTaskSending(false);
    }
  }

  async function refreshTaskSnapshot(worktree: Worktree) {
    if (connection.kind !== "connected") return;
    setTaskSnapshotLoadingId(worktree.id);
    setTaskSnapshotErrors((current) => ({ ...current, [worktree.id]: "" }));
    try {
      const snapshot = await invoke<SessionSnapshot>("refresh_worktree_session", {
        baseUrl,
        username,
        password,
        worktreeId: worktree.id,
      });
      setTaskSnapshots((current) => ({ ...current, [worktree.id]: snapshot }));
    } catch (error) {
      setTaskSnapshotErrors((current) => ({ ...current, [worktree.id]: String(error) }));
    } finally {
      setTaskSnapshotLoadingId(null);
    }
  }

  async function answerPermission(worktree: Worktree, permission: PendingPermission, allowOnce: boolean) {
    if (connection.kind !== "connected") return;
    setPermissionBusyId(permission.id);
    setTaskSnapshotErrors((current) => ({ ...current, [worktree.id]: "" }));
    try {
      await invoke("reply_to_worktree_permission", {
        baseUrl,
        username,
        password,
        worktreeId: worktree.id,
        permissionId: permission.id,
        allowOnce,
      });
      await refreshTaskSnapshot(worktree);
    } catch (error) {
      setTaskSnapshotErrors((current) => ({ ...current, [worktree.id]: String(error) }));
    } finally {
      setPermissionBusyId(null);
    }
  }

  function resetConnectionState() {
    setConnection({ kind: "idle" });
    setCatalogState({ kind: "idle" });
  }

  async function loadCatalog(
    serverBaseUrl = baseUrl,
    serverUsername = username,
    serverPassword = password,
  ) {
    setCatalogState({ kind: "loading" });

    try {
      const catalog = await invoke<OpenCodeCatalog>("discover_opencode_catalog", {
        baseUrl: serverBaseUrl,
        username: serverUsername,
        password: serverPassword,
      });
      setCatalogState({ kind: "loaded", catalog });
    } catch (error) {
      setCatalogState({ kind: "error", message: String(error) });
    }
  }

  async function startManagedOpenCodeServer() {
    setManagedServerState("starting");
    setConnection({ kind: "checking" });
    setCatalogState({ kind: "idle" });

    try {
      const server = await invoke<StartedOpenCodeServer>("start_managed_opencode_server", { baseUrl });
      setBaseUrl(server.baseUrl);
      setUsername(server.username);
      setPassword(server.password);
      setManagedServerState("running");
      setConnection({ kind: "connected", version: server.version });
      await loadCatalog(server.baseUrl, server.username, server.password);
    } catch (error) {
      setManagedServerState("stopped");
      setConnection({ kind: "error", message: String(error) });
    }
  }

  async function stopManagedOpenCodeServer() {
    if (!window.confirm("¿Detener el servidor OpenCode iniciado por Stade Studio? Las sesiones activas perderán la conexión.")) return;
    setManagedServerState("stopping");
    try {
      await invoke("stop_managed_opencode_server");
      setManagedServerState("stopped");
      setPassword("");
      setConnection({ kind: "idle" });
      setCatalogState({ kind: "idle" });
    } catch (error) {
      setManagedServerState("running");
      setConnection({ kind: "error", message: String(error) });
    }
  }

  async function checkConnection() {
    setConnection({ kind: "checking" });
    setCatalogState({ kind: "idle" });

    try {
      const health = await invoke<OpenCodeHealth>("check_opencode_connection", {
        baseUrl,
        username,
        password,
      });
      setConnection({ kind: "connected", version: health.version });
      await loadCatalog();
    } catch (error) {
      setConnection({ kind: "error", message: String(error) });
    }
  }

  const isChecking = connection.kind === "checking";
  const isCatalogLoading = catalogState.kind === "loading";
  const connectionFieldsDisabled = isChecking || isCatalogLoading || managedServerState !== "stopped";

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true"><img src={brandLogo} alt="" /></div>
          <div>
            <p className="brand-name">Stade Studio</p>
            <p className="brand-caption">ESTUDIO LOCAL DE AGENTES</p>
          </div>
        </div>

        <div className="workspace-label">ESPACIO DE TRABAJO</div>
        <nav className="navigation" aria-label="Navegación principal">
          <button className={`nav-item ${page === "dashboard" ? "active" : ""}`} type="button" onClick={() => setPage("dashboard")}>
            <span className="nav-icon grid-icon" aria-hidden="true" />
            <span>Panel</span>
            {page === "dashboard" && <span className="nav-indicator" />}
          </button>
          <button className={`nav-item ${page === "projects" ? "active" : ""}`} type="button" onClick={() => setPage("projects")}>
            <span className="nav-icon folder-icon" aria-hidden="true" />
            <span>Proyectos</span>
            {page === "projects" && <span className="nav-indicator" />}
          </button>
          <button className="nav-item" type="button" disabled>
            <span className="nav-icon activity-icon" aria-hidden="true" />
            <span>Actividad</span>
          </button>
        </nav>

        <div className="sidebar-footer">
          <div className="local-indicator"><span /> Solo en este equipo</div>
          <div className="sidebar-version">PREVIEW · 0.1.0</div>
        </div>
      </aside>

      <main className="main-content">
        <header className="topbar">
          <div className="breadcrumb"><span>Stade Studio</span><i>/</i><strong>{page === "dashboard" ? "Resumen" : "Proyectos"}</strong></div>
          <div className="local-badge"><span className="local-badge-dot" /> DATOS LOCALES</div>
        </header>

        {page === "dashboard" ? (
          <>
        <section className="page-heading">
          <div>
            <p className="eyebrow">TU ESPACIO DE TRABAJO</p>
            <h1>Coordina a tus agentes.</h1>
            <p className="page-description">
              Proyectos, modelos y sesiones de OpenCode en un solo lugar.
            </p>
          </div>
        </section>

        <section className="overview-grid" aria-label="Estado del espacio de trabajo">
          <article className="metric-card">
            <div className="metric-label"><span className="metric-dot purple" /> PROYECTOS</div>
            <div className="metric-value">{projectsLoading ? "…" : projects.filter((project) => project.archivedAt === null).length}</div>
            <div className="metric-footnote">Registrados en este equipo</div>
          </article>
          <article className="metric-card">
            <div className="metric-label"><span className="metric-dot blue" /> PERFILES</div>
            <div className="metric-value">{catalogState.kind === "loaded" ? catalogState.catalog.agents?.length ?? "—" : "—"}</div>
            <div className="metric-footnote">Perfiles descubiertos en OpenCode</div>
          </article>
          <article className="metric-card connection-metric">
            <div className="metric-label"><span className="metric-dot green" /> OPENCODE</div>
            <div className="metric-value metric-status">
              <span className={`status-indicator ${connection.kind}`} />
              {connection.kind === "connected" ? "Conectado" : connection.kind === "checking" ? "Conectando" : connection.kind === "error" ? "Sin conexión" : "Sin probar"}
            </div>
            <div className="metric-footnote">
              {connection.kind === "connected" ? `Versión ${connection.version}` : "Servidor local · puerto 4096"}
            </div>
          </article>
        </section>

        <section className="content-grid">
          <article className="panel connection-panel">
            <div className="panel-heading">
              <div>
                <p className="eyebrow">PRIMER PASO</p>
                <h2>Conecta OpenCode</h2>
              </div>
              <div className="panel-symbol" aria-hidden="true">↗</div>
            </div>
            <p className="panel-description">
              Comprueba el servidor local de OpenCode. La aplicación solo se conecta a direcciones de loopback.
            </p>

            <form
              className="connection-form"
              onSubmit={(event) => {
                event.preventDefault();
                void checkConnection();
              }}
            >
              <label htmlFor="opencode-url">URL del servidor</label>
              <div className="input-row">
                <div className="input-shell">
                  <span className="input-prefix" aria-hidden="true">⌘</span>
                  <input
                    id="opencode-url"
                    type="url"
                    value={baseUrl}
                    disabled={connectionFieldsDisabled}
                    onChange={(event) => {
                      setBaseUrl(event.currentTarget.value);
                      resetConnectionState();
                    }}
                    placeholder="http://127.0.0.1:4096"
                    spellCheck={false}
                    autoComplete="url"
                  />
                </div>
                <button className="primary-button" type="submit" disabled={connectionFieldsDisabled || !baseUrl.trim()}>
                  {isChecking || isCatalogLoading ? <><span className="spinner" /> {isChecking ? "Conectando" : "Descubriendo"}</> : "Conectar y descubrir"}
                </button>
              </div>

              <div className="credentials-row">
                <div className="credential-field username-field">
                  <label htmlFor="opencode-username">Usuario</label>
                  <input
                    id="opencode-username"
                    value={username}
                    disabled={connectionFieldsDisabled}
                    onChange={(event) => {
                      setUsername(event.currentTarget.value);
                      resetConnectionState();
                    }}
                    autoComplete="username"
                  />
                </div>
                <div className="credential-field password-field">
                  <label htmlFor="opencode-password">Contraseña del servidor</label>
                  <input
                    id="opencode-password"
                    type="password"
                    value={password}
                    disabled={connectionFieldsDisabled}
                    onChange={(event) => {
                      setPassword(event.currentTarget.value);
                      resetConnectionState();
                    }}
                    placeholder="Pega la contraseña temporal de OpenCode"
                    autoComplete="current-password"
                  />
                </div>
              </div>
              <p className="credential-note">Solo se mantiene en memoria mientras la aplicación está abierta.</p>
              <div className="managed-server-row">
                <div className="managed-server-copy">
                  <strong>{managedServerState === "running" ? "OpenCode iniciado desde Stade Studio" : "Sin consola adicional"}</strong>
                  <span>{managedServerState === "running"
                    ? `Servidor local activo en ${baseUrl}; la contraseña temporal se capturó automáticamente.`
                    : "Inicia OpenCode aquí y la app se conectará y descubrirá el catálogo automáticamente."}</span>
                </div>
                {managedServerState === "running" ? (
                  <button className="stop-server-button" type="button" onClick={() => void stopManagedOpenCodeServer()}>
                    Detener servidor
                  </button>
                ) : (
                  <button
                    className="managed-server-button"
                    type="button"
                    onClick={() => void startManagedOpenCodeServer()}
                    disabled={managedServerState !== "stopped" || isChecking || isCatalogLoading || !baseUrl.trim()}
                  >
                    {managedServerState === "starting" ? <><span className="spinner" /> Iniciando…</> : managedServerState === "stopping" ? "Deteniendo…" : "Iniciar y conectar"}
                  </button>
                )}
              </div>
            </form>

            {connection.kind === "connected" && (
              <div className="feedback success-feedback" role="status" aria-live="polite">
                <span className="feedback-icon">✓</span>
                <div><strong>Conexión establecida</strong><span>OpenCode {connection.version} responde correctamente.</span></div>
              </div>
            )}
            {connection.kind === "error" && (
              <div className="feedback error-feedback" role="alert">
                <span className="feedback-icon">!</span>
                <div><strong>No se pudo conectar</strong><span>{connection.message}</span></div>
              </div>
            )}
            {connection.kind === "connected" && catalogState.kind === "error" && (
              <div className="feedback error-feedback" role="alert">
                <span className="feedback-icon">!</span>
                <div><strong>No se pudo cargar el catálogo</strong><span>{catalogState.message}</span></div>
              </div>
            )}
            {connection.kind === "connected" && (
              <div className="catalog-action-row">
                <span>{isCatalogLoading ? "Consultando perfiles y modelos de OpenCode…" : "Perfiles y modelos disponibles en esta instalación"}</span>
                <button className="secondary-button" type="button" onClick={() => void loadCatalog()} disabled={isCatalogLoading}>
                  {catalogState.kind === "loaded" || catalogState.kind === "error" ? "Actualizar catálogo" : "Descubrir catálogo"}
                </button>
              </div>
            )}

            <div className="panel-divider" />
            <div className="connection-hint">
              <span className="hint-icon">i</span>
              <span>La app puede iniciar OpenCode en segundo plano o conectarse a un proceso local existente. Las conexiones se limitan a loopback.</span>
            </div>
          </article>

          <aside className="panel roadmap-panel">
            <p className="eyebrow">EN CONSTRUCCIÓN</p>
            <h2>Un flujo, varios especialistas.</h2>
            <p className="panel-description">
              Estamos preparando el espacio compartido para que cada agente tome su parte y entregue el trabajo al siguiente.
            </p>
            <div className="roadmap-list">
              <div className="roadmap-item done"><span className="roadmap-check">✓</span><span>Ventana de escritorio local</span></div>
              <div className="roadmap-item done"><span className="roadmap-check">✓</span><span>Conexión y catálogo OpenCode</span></div>
              <div className="roadmap-item done"><span className="roadmap-check">✓</span><span>Proyectos y worktrees Git</span></div>
              <div className="roadmap-item done"><span className="roadmap-check">✓</span><span>Sesión OpenCode por entorno</span></div>
              <div className="roadmap-item current"><span className="roadmap-pulse" /><span>Tareas, permisos y entregas</span><span className="roadmap-tag">SIGUIENTE</span></div>
            </div>
            <div className="privacy-note"><span className="lock-icon" aria-hidden="true">▣</span> Tus proyectos permanecen en este equipo.</div>
          </aside>
        </section>

        {catalogState.kind === "loaded" && (
          <section className="catalog-panel panel" aria-live="polite">
            <div className="catalog-header">
              <div>
                <p className="eyebrow">DESCUBIERTO EN OPENCODE</p>
                <h2>Perfiles y modelos disponibles</h2>
              </div>
              <button className="secondary-button" type="button" onClick={() => void loadCatalog()} disabled={isCatalogLoading}>
                Actualizar
              </button>
            </div>

            {catalogState.catalog.warnings.length > 0 && (
              <div className="catalog-warnings" role="status">
                {catalogState.catalog.warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </div>
            )}

            <div className="catalog-columns">
              <div className="catalog-group">
                <div className="catalog-group-heading">
                  <h3>Perfiles de agente</h3>
                  <span>{catalogState.catalog.agents?.length ?? "—"}</span>
                </div>
                {catalogState.catalog.agents === null ? (
                  <p className="catalog-empty">No se pudieron consultar los perfiles.</p>
                ) : catalogState.catalog.agents.length === 0 ? (
                  <p className="catalog-empty">OpenCode no devolvió perfiles para esta ubicación.</p>
                ) : (
                  <div className="catalog-list">
                    {catalogState.catalog.agents.map((agent) => (
                      <article className="catalog-item" key={agent.id}>
                        <div className="catalog-item-title">
                          <strong>{agent.name}</strong>
                          <span className="catalog-tag">{agent.mode ?? "agente"}{agent.hidden ? " · oculto" : ""}</span>
                        </div>
                        <p>{agent.description || `ID: ${agent.id}`}</p>
                      </article>
                    ))}
                  </div>
                )}
              </div>

              <div className="catalog-group">
                <div className="catalog-group-heading">
                  <h3>Modelos</h3>
                  <span>{catalogState.catalog.models?.length ?? "—"}</span>
                </div>
                {catalogState.catalog.models === null ? (
                  <p className="catalog-empty">No se pudieron consultar los modelos en esta versión de OpenCode.</p>
                ) : catalogState.catalog.models.length === 0 ? (
                  <p className="catalog-empty">OpenCode no devolvió modelos para esta instalación.</p>
                ) : (
                  <div className="catalog-list">
                    {catalogState.catalog.models.map((model) => (
                      <article className="catalog-item" key={`${model.providerId ?? "provider"}/${model.modelId ?? model.id}`}>
                        <div className="catalog-item-title">
                          <strong>{model.name}</strong>
                          <span className={`catalog-tag ${model.enabled === true ? "tag-enabled" : model.enabled === false ? "tag-disabled" : ""}`}>
                            {model.enabled === true ? "disponible" : model.enabled === false ? "desactivado" : "estado desconocido"}
                          </span>
                        </div>
                        <p>{model.providerId ?? "Proveedor desconocido"} · {model.modelId ?? model.id}</p>
                      </article>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </section>
        )}

          </>
        ) : (
          <section className="projects-page">
            <div className="page-heading projects-page-heading">
              <div>
                <p className="eyebrow">ESPACIOS DE TRABAJO LOCALES</p>
                <h1>Proyectos</h1>
                <p className="page-description">Registra carpetas de proyecto en este equipo. No se modifica su contenido.</p>
              </div>
              <button className="primary-button add-project-button" type="button" onClick={() => openProjectDialog()}>
                <span aria-hidden="true">＋</span> Añadir proyecto
              </button>
            </div>

            <div className="projects-toolbar">
              <span>{projectsLoading ? "Cargando proyectos…" : `${projects.filter((project) => project.archivedAt === null).length} proyectos activos`}</span>
              <div className="projects-toolbar-actions">
                <label className="archived-toggle">
                  <input
                    type="checkbox"
                    checked={showArchivedProjects}
                    onChange={(event) => {
                      const includeArchived = event.currentTarget.checked;
                      setShowArchivedProjects(includeArchived);
                      void refreshProjects(includeArchived);
                    }}
                  />
                  <span>Incluir archivados</span>
                </label>
                <button className="secondary-button" type="button" onClick={() => void refreshProjects(showArchivedProjects)} disabled={projectsLoading}>
                  Actualizar
                </button>
              </div>
            </div>

            {projectsError && <div className="feedback error-feedback project-list-error" role="alert"><span className="feedback-icon">!</span><div><strong>Error con proyectos</strong><span>{projectsError}</span></div></div>}

            {projectsLoading ? (
              <div className="project-empty-state">Cargando la lista local…</div>
            ) : projects.filter((project) => showArchivedProjects || project.archivedAt === null).length === 0 ? (
              <div className="project-empty-state">
                <span className="empty-folder-icon" aria-hidden="true">▱</span>
                <h2>{showArchivedProjects ? "Todavía no hay proyectos" : "Aún no has añadido proyectos"}</h2>
                <p>Elige la carpeta raíz de un proyecto. La app solo registra la ruta; no copia ni modifica archivos.</p>
                <button className="secondary-button" type="button" onClick={() => openProjectDialog()}>Añadir primer proyecto</button>
              </div>
            ) : (
              <div className="project-list">
                {projects.filter((project) => showArchivedProjects || project.archivedAt === null).map((project) => (
                  <article className={`project-card ${project.archivedAt ? "project-archived" : ""}`} key={project.id}>
                    <div className="project-card-symbol" aria-hidden="true"><span className="folder-icon" /></div>
                    <div className="project-card-content">
                      <div className="project-card-title-row">
                        <h2>{project.name}</h2>
                        <span className={`project-git-badge ${project.isGitRepository ? "git-ready" : "git-missing"}`}>
                          {project.isGitRepository ? "REPOSITORIO GIT" : "GIT NECESARIO PARA WORKTREES"}
                        </span>
                        {project.archivedAt && <span className="project-archived-badge">ARCHIVADO</span>}
                      </div>
                      <code className="project-root-path">{project.rootPath}</code>
                      <p className="project-description">{project.description || "Sin descripción"}</p>
                    </div>
                    <div className="project-card-actions">
                      {!project.archivedAt && <button className="project-action-button" type="button" onClick={() => openProjectDialog(project)}>Editar</button>}
                      <button className="project-action-button" type="button" onClick={() => toggleProjectEnvironments(project)}>
                        {expandedProjectId === project.id ? "Ocultar entornos" : `Entornos${worktreesByProject[project.id] ? ` · ${worktreesByProject[project.id].length}` : ""}`}
                      </button>
                      <button className="project-action-button" type="button" onClick={() => void toggleProjectArchived(project)} disabled={projectBusyId === project.id}>
                        {projectBusyId === project.id ? "Guardando…" : project.archivedAt ? "Restaurar" : "Archivar"}
                      </button>
                    </div>
                    {expandedProjectId === project.id && (
                      <div className="project-worktrees-panel">
                        <div className="worktrees-panel-header">
                          <div>
                            <h3>Entornos aislados</h3>
                            <p>Un directorio de trabajo y una rama Git independientes por agente/tarea.</p>
                          </div>
                          {project.isGitRepository && !project.archivedAt && (
                            <button className="secondary-button" type="button" onClick={() => openWorktreeDialog(project)}>
                              Crear entorno
                            </button>
                          )}
                        </div>
                        {!project.isGitRepository && <p className="worktree-inline-note">Registra la raíz de un repositorio Git para crear entornos.</p>}
                        {project.archivedAt && <p className="worktree-inline-note">Restaura el proyecto antes de crear nuevos entornos.</p>}
                        {worktreesErrors[project.id] && <p className="worktree-inline-error" role="alert">{worktreesErrors[project.id]}</p>}
                        {worktreesLoadingProjectId === project.id ? (
                          <p className="worktree-list-empty">Cargando entornos…</p>
                        ) : (worktreesByProject[project.id] ?? []).length === 0 ? (
                          <p className="worktree-list-empty">Aún no hay entornos registrados para este proyecto.</p>
                        ) : (
                          <div className="worktree-list">
                            {(worktreesByProject[project.id] ?? []).map((worktree) => (
                              <article className="worktree-item" key={worktree.id}>
                                <div className="worktree-item-heading">
                                  <strong>{worktree.label}</strong>
                                  <span className={`worktree-status status-${worktree.status}`}>
                                    {worktree.status === "ready" ? "LISTO" : worktree.status === "creating" ? "PREPARANDO" : "REVISAR"}
                                  </span>
                                </div>
                                <p className="worktree-branch">{worktree.branchName}</p>
                                <code>{worktree.directory}</code>
                                {worktree.lastError && <p className="worktree-inline-error">{worktree.lastError}</p>}
                                {worktree.opencodeSessionId && (
                                  <div className="worktree-session-details">
                                    <div className="worktree-item-heading">
                                      <strong>Sesión {worktree.opencodeSessionId}</strong>
                                      <span className={`worktree-status ${worktree.opencodeLocationMatches ? "status-ready" : "status-failed"}`}>
                                        {worktree.opencodeLocationMatches ? "UBICACIÓN VERIFICADA" : "UBICACIÓN NO VERIFICADA"}
                                      </span>
                                    </div>
                                    <p>Perfil: {worktree.opencodeAgentId} · Modelo: {worktree.opencodeProviderId}/{worktree.opencodeModelId}</p>
                                    {worktree.opencodeLocationDirectory && <code>{worktree.opencodeLocationDirectory}</code>}
                  <p className="worktree-inline-note">Stade Studio conserva el ID y la asignación; no guarda el texto de los prompts.</p>
                                  </div>
                                )}
                                {worktree.opencodeSessionId && worktree.opencodeLocationMatches === true && (
                                  <div className="worktree-task-panel">
                                    <div className="worktree-task-toolbar">
                                      <div className="worktree-task-status">
                                        <span className={`worktree-status ${taskSnapshots[worktree.id]?.active ? "status-running" : taskSnapshots[worktree.id]?.permissions.length ? "status-waiting" : ""}`}>
                                          {taskSnapshots[worktree.id]?.permissions.length
                                            ? "REQUIERE APROBACIÓN"
                                            : taskSnapshots[worktree.id]?.active
                                              ? "TRABAJANDO"
                                              : taskSnapshots[worktree.id]?.latestResponse
                                                ? taskSnapshots[worktree.id]?.responseCompleted ? "RESPUESTA RECIBIDA" : "RESPONDIENDO"
                                                : "SESIÓN LISTA"}
                                        </span>
                                        {taskAcceptedAt[worktree.id] && <span>Último envío: {taskAcceptedAt[worktree.id]}</span>}
                                      </div>
                                      <div className="worktree-task-buttons">
                                        <button className="secondary-button" type="button" onClick={() => openTaskDialog(worktree)} disabled={connection.kind !== "connected"}>
                                          Enviar tarea
                                        </button>
                                        <button className="project-action-button" type="button" onClick={() => void refreshTaskSnapshot(worktree)} disabled={connection.kind !== "connected" || taskSnapshotLoadingId === worktree.id}>
                                          {taskSnapshotLoadingId === worktree.id ? "Actualizando…" : "Actualizar estado"}
                                        </button>
                                      </div>
                                    </div>
                                    {connection.kind !== "connected" && <p className="worktree-inline-note">Conecta OpenCode para enviar tareas y consultar el estado.</p>}
                                    {taskSnapshotErrors[worktree.id] && <p className="worktree-inline-error" role="alert">{taskSnapshotErrors[worktree.id]}</p>}
                                    {taskSnapshots[worktree.id]?.permissionWarning && <p className="worktree-inline-note">No se pudieron consultar aprobaciones: {taskSnapshots[worktree.id]?.permissionWarning}</p>}
                                    {(taskSnapshots[worktree.id]?.permissions ?? []).map((permission) => (
                                      <div className="permission-request-card" key={permission.id}>
                                        <div className="worktree-item-heading">
                                          <strong>Permiso solicitado: {permission.action}</strong>
                                          <span className="worktree-status status-waiting">PENDIENTE</span>
                                        </div>
                                        {permission.message && <p>{permission.message}</p>}
                                        <div className="permission-resources">
                                          {permission.resources.map((resource) => <code key={resource}>{resource}</code>)}
                                        </div>
                                        <div className="permission-actions">
                                          <button className="secondary-button" type="button" onClick={() => void answerPermission(worktree, permission, true)} disabled={permissionBusyId === permission.id}>
                                            {permissionBusyId === permission.id ? "Enviando…" : "Permitir una vez"}
                                          </button>
                                          <button className="project-action-button" type="button" onClick={() => void answerPermission(worktree, permission, false)} disabled={permissionBusyId === permission.id}>
                                            Rechazar
                                          </button>
                                          <span>El rechazo descarta todas las solicitudes pendientes de esta sesión.</span>
                                        </div>
                                      </div>
                                    ))}
                                    {taskSnapshots[worktree.id]?.latestResponse && (
                                      <div className="agent-response-card">
                                        <div className="worktree-item-heading"><strong>Última respuesta del agente</strong></div>
                                        <div className="agent-response-text">{taskSnapshots[worktree.id].latestResponse}</div>
                                      </div>
                                    )}
                                    <p className="worktree-safety-note">El prompt se envía a OpenCode y no se copia a SQLite. Las aprobaciones desde aquí son siempre de una sola vez.</p>
                                  </div>
                                )}
                                {worktree.status === "ready" && !worktree.opencodeSessionId && (
                                  <div className="worktree-session-action">
                                    <button
                                      className="secondary-button"
                                      type="button"
                                      onClick={() => openSessionDialog(worktree)}
                                      disabled={connection.kind !== "connected" || catalogState.kind !== "loaded"}
                                    >
                                      Crear sesión OpenCode
                                    </button>
                                    <span>{connection.kind === "connected" && catalogState.kind === "loaded" ? "Elige un perfil y un modelo; no se ejecutará trabajo todavía." : "Conecta OpenCode y descubre perfiles/modelos primero."}</span>
                                  </div>
                                )}
                              </article>
                            ))}
                          </div>
                        )}
                        <p className="worktree-safety-note">Los worktrees separan los cambios, pero no son un aislamiento de seguridad de Windows. Las sesiones solo se ejecutan cuando envías una tarea.</p>
                      </div>
                    )}
                  </article>
                ))}
              </div>
            )}
          </section>
        )}

        {projectDialogOpen && (
          <div className="project-modal-backdrop">
            <section className="project-modal" role="dialog" aria-modal="true" aria-labelledby="project-dialog-title">
              <div className="project-modal-header">
                <div>
                  <p className="eyebrow">PROYECTO LOCAL</p>
                  <h2 id="project-dialog-title">{editingProject ? "Editar proyecto" : "Añadir proyecto"}</h2>
                </div>
                <button className="modal-close-button" type="button" onClick={() => setProjectDialogOpen(false)} aria-label="Cerrar" disabled={projectSaving}>×</button>
              </div>
              <form className="project-form" onSubmit={(event) => void saveProject(event)}>
                <label htmlFor="project-name">Nombre</label>
                <input
                  id="project-name"
                  value={projectForm.name}
                  onChange={(event) => {
                    const name = event.currentTarget.value;
                    setProjectForm((current) => ({ ...current, name }));
                  }}
                  maxLength={100}
                  required
                  autoFocus
                  disabled={projectSaving}
                  placeholder="Mi proyecto"
                />
                <label htmlFor="project-root">Carpeta raíz</label>
                <div className="project-path-row">
                  <input
                    id="project-root"
                    value={projectForm.rootPath}
                    onChange={(event) => {
                      const rootPath = event.currentTarget.value;
                      setProjectForm((current) => ({ ...current, rootPath }));
                    }}
                    required
                    readOnly={Boolean(editingProject)}
                    disabled={projectSaving}
                    placeholder="Selecciona o pega una ruta absoluta"
                    spellCheck={false}
                  />
                  {!editingProject && <button className="secondary-button" type="button" onClick={() => void chooseProjectDirectory()} disabled={projectSaving}>Examinar…</button>}
                </div>
                <p className="project-form-hint">Selecciona la raíz del repositorio Git para poder crear worktrees aislados más adelante. Registrar el proyecto no cambia sus archivos.</p>
                <label htmlFor="project-description">Descripción <span>(opcional)</span></label>
                <textarea
                  id="project-description"
                  value={projectForm.description}
                  onChange={(event) => {
                    const description = event.currentTarget.value;
                    setProjectForm((current) => ({ ...current, description }));
                  }}
                  maxLength={500}
                  rows={3}
                  disabled={projectSaving}
                  placeholder="¿En qué consiste este proyecto?"
                />
                {projectFormError && <div className="project-form-error" role="alert">{projectFormError}</div>}
                <div className="project-form-actions">
                  <button className="secondary-button" type="button" onClick={() => setProjectDialogOpen(false)} disabled={projectSaving}>Cancelar</button>
                  <button className="primary-button" type="submit" disabled={projectSaving || !projectForm.name.trim() || (!editingProject && !projectForm.rootPath.trim())}>
                    {projectSaving ? "Guardando…" : editingProject ? "Guardar cambios" : "Registrar proyecto"}
                  </button>
                </div>
              </form>
            </section>
          </div>
        )}

        {worktreeDialogProject && (
          <div className="project-modal-backdrop">
            <section className="project-modal" role="dialog" aria-modal="true" aria-labelledby="worktree-dialog-title">
              <div className="project-modal-header">
                <div>
                  <p className="eyebrow">{worktreeDialogProject.name}</p>
                  <h2 id="worktree-dialog-title">Crear entorno aislado</h2>
                </div>
                <button className="modal-close-button" type="button" onClick={() => setWorktreeDialogProject(null)} aria-label="Cerrar" disabled={worktreeSaving}>×</button>
              </div>
              <form className="project-form" onSubmit={(event) => void saveWorktree(event)}>
                <label htmlFor="worktree-label">Nombre del agente o tarea</label>
                <input
                  id="worktree-label"
                  value={worktreeLabel}
                  onChange={(event) => setWorktreeLabel(event.currentTarget.value)}
                  maxLength={80}
                  required
                  autoFocus
                  disabled={worktreeSaving}
                  placeholder="Backend, frontend, revisión…"
                />
                <div className="worktree-create-explanation">
                  <p>Se creará una rama única desde el último commit y un directorio separado bajo los datos locales de Stade Studio.</p>
                  <p>Por seguridad, el repositorio debe estar limpio. Guarda o confirma tus cambios antes de continuar.</p>
                </div>
                {worktreeFormError && <div className="project-form-error" role="alert">{worktreeFormError}</div>}
                <div className="project-form-actions">
                  <button className="secondary-button" type="button" onClick={() => setWorktreeDialogProject(null)} disabled={worktreeSaving}>Cancelar</button>
                  <button className="primary-button" type="submit" disabled={worktreeSaving || !worktreeLabel.trim()}>
                    {worktreeSaving ? "Creando entorno…" : "Crear entorno"}
                  </button>
                </div>
              </form>
            </section>
          </div>
        )}

        {sessionDialogWorktree && (
          <div className="project-modal-backdrop">
            <section className="project-modal" role="dialog" aria-modal="true" aria-labelledby="session-dialog-title">
              <div className="project-modal-header">
                <div>
                  <p className="eyebrow">{sessionDialogWorktree.label}</p>
                  <h2 id="session-dialog-title">Crear sesión en este entorno</h2>
                </div>
                <button className="modal-close-button" type="button" onClick={() => setSessionDialogWorktree(null)} aria-label="Cerrar" disabled={sessionSaving}>×</button>
              </div>
              <form className="project-form" onSubmit={(event) => void createOpenCodeSession(event)}>
                <label htmlFor="session-agent">Perfil OpenCode</label>
                <select
                  id="session-agent"
                  value={selectedAgentId}
                  onChange={(event) => setSelectedAgentId(event.currentTarget.value)}
                  required
                  disabled={sessionSaving || selectableAgents.length === 0}
                >
                  {selectableAgents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name} · {agent.id}</option>)}
                </select>
                {selectableAgents.length === 0 && <p className="project-form-hint">No se encontró un perfil primario disponible en el catálogo conectado.</p>}

                <label htmlFor="session-model">Modelo</label>
                <select
                  id="session-model"
                  value={selectedModelIndex}
                  onChange={(event) => setSelectedModelIndex(event.currentTarget.value)}
                  required
                  disabled={sessionSaving || selectableModels.length === 0}
                >
                  {selectableModels.map((model, index) => <option key={`${model.providerId}/${model.id}`} value={String(index)}>{model.name} · {model.providerId}/{model.modelId ?? model.id}</option>)}
                </select>
                {selectableModels.length === 0 && <p className="project-form-hint">No se encontró un modelo habilitado con proveedor disponible.</p>}

                <div className="worktree-create-explanation">
                  <p>OpenCode recibirá la ruta exacta de este worktree. Stade Studio comprobará la ubicación devuelta y la marcará si no coincide.</p>
                  <p>Esto solo crea una sesión vacía: aún no se envía prompt ni se ejecuta el modelo.</p>
                </div>
                {sessionFormError && <div className="project-form-error" role="alert">{sessionFormError}</div>}
                <div className="project-form-actions">
                  <button className="secondary-button" type="button" onClick={() => setSessionDialogWorktree(null)} disabled={sessionSaving}>Cancelar</button>
                  <button className="primary-button" type="submit" disabled={sessionSaving || selectableAgents.length === 0 || selectableModels.length === 0}>
                    {sessionSaving ? "Creando sesión…" : "Crear sesión vacía"}
                  </button>
                </div>
              </form>
            </section>
          </div>
        )}

        {taskDialogWorktree && (
          <div className="project-modal-backdrop">
            <section className="project-modal task-prompt-modal" role="dialog" aria-modal="true" aria-labelledby="task-dialog-title">
              <div className="project-modal-header">
                <div>
                  <p className="eyebrow">{taskDialogWorktree.label} · {taskDialogWorktree.opencodeAgentId}</p>
                  <h2 id="task-dialog-title">Enviar tarea al agente</h2>
                </div>
                <button className="modal-close-button" type="button" onClick={() => setTaskDialogWorktree(null)} aria-label="Cerrar" disabled={taskSending}>×</button>
              </div>
              <form className="project-form" onSubmit={(event) => void sendTaskPrompt(event)}>
                <label htmlFor="task-prompt">Instrucciones</label>
                <textarea
                  id="task-prompt"
                  value={taskPrompt}
                  onChange={(event) => setTaskPrompt(event.currentTarget.value)}
                  maxLength={20_000}
                  rows={8}
                  required
                  autoFocus
                  disabled={taskSending}
                  placeholder="Describe el objetivo, las restricciones y el resultado esperado…"
                />
                <div className="task-prompt-notice">
                  <p>El texto se envía a OpenCode y Stade Studio no lo guarda en SQLite.</p>
                  <p>Si esta sesión ya está trabajando, el mensaje se pone en cola; no interrumpe lo que está haciendo.</p>
                  <p>Las solicitudes de permisos aparecerán en el entorno y se pueden aprobar solo una vez o rechazar.</p>
                </div>
                {taskFormError && <div className="project-form-error" role="alert">{taskFormError}</div>}
                <div className="project-form-actions">
                  <button className="secondary-button" type="button" onClick={() => setTaskDialogWorktree(null)} disabled={taskSending}>Cancelar</button>
                  <button className="primary-button" type="submit" disabled={taskSending || !taskPrompt.trim()}>
                    {taskSending ? "Enviando a OpenCode…" : "Enviar tarea"}
                  </button>
                </div>
              </form>
            </section>
          </div>
        )}

        <footer className="page-footer">
          <span>Stade Studio</span>
          <span>Proyectos y agentes, cada uno en su espacio.</span>
        </footer>
      </main>
    </div>
  );
}

export default App;
