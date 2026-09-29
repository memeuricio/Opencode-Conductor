import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDirectoryDialog } from "@tauri-apps/plugin-dialog";
import brandLogo from "./assets/logo.webp";
import "./App.css";
import "./brand.css";
import UsagePage from "./usage/UsagePage";
import TasksPage from "./tasks/TasksPage";

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

interface Workspace {
  id: number;
  name: string;
  description: string | null;
  projectIds: number[];
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

interface EventStreamStatus {
  state: "connecting" | "connected" | "reconnecting" | "error" | "stopped";
  retryInMs: number | null;
  message: string | null;
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
  const [page, setPage] = useState<"dashboard" | "projects" | "tasks" | "usage">("dashboard");
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
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspacesLoading, setWorkspacesLoading] = useState(true);
  const [workspaceFilter, setWorkspaceFilter] = useState<number | "none" | null>(null);
  const [showArchivedWorkspaces, setShowArchivedWorkspaces] = useState(false);
  const [newWorkspaceName, setNewWorkspaceName] = useState("");
  const [workspaceBusy, setWorkspaceBusy] = useState(false);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);
  const [editingWorkspaceId, setEditingWorkspaceId] = useState<number | null>(null);
  const [editingWorkspaceName, setEditingWorkspaceName] = useState("");
  const [membershipBusyKey, setMembershipBusyKey] = useState<string | null>(null);
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
  const [eventStreamStatus, setEventStreamStatus] = useState<EventStreamStatus>({
    state: "stopped",
    retryInMs: null,
    message: null,
  });
  const [lastRealtimeSync, setLastRealtimeSync] = useState<number | null>(null);
  const knownWorktreesRef = useRef<Worktree[]>([]);
  const taskSnapshotsRef = useRef(taskSnapshots);
  knownWorktreesRef.current = Object.values(worktreesByProject).flat();
  taskSnapshotsRef.current = taskSnapshots;

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
    void refreshWorkspaces(false);
  }, []);

  async function refreshWorkspaces(includeArchived: boolean) {
    setWorkspacesLoading(true);
    setWorkspaceError(null);
    try {
      const result = await invoke<Workspace[]>("list_workspaces", { includeArchived });
      setWorkspaces(result);
    } catch (error) {
      setWorkspaceError(String(error));
    } finally {
      setWorkspacesLoading(false);
    }
  }

  async function createWorkspace(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!newWorkspaceName.trim()) return;
    setWorkspaceBusy(true);
    setWorkspaceError(null);
    try {
      const created = await invoke<Workspace>("create_workspace", {
        name: newWorkspaceName,
        description: null,
      });
      setNewWorkspaceName("");
      await refreshWorkspaces(showArchivedWorkspaces);
      setWorkspaceFilter(created.id);
    } catch (error) {
      setWorkspaceError(String(error));
    } finally {
      setWorkspaceBusy(false);
    }
  }

  async function renameWorkspace(workspace: Workspace) {
    if (!editingWorkspaceName.trim()) return;
    setWorkspaceBusy(true);
    setWorkspaceError(null);
    try {
      await invoke<Workspace>("update_workspace", {
        workspaceId: workspace.id,
        name: editingWorkspaceName,
        description: workspace.description,
      });
      setEditingWorkspaceId(null);
      await refreshWorkspaces(showArchivedWorkspaces);
    } catch (error) {
      setWorkspaceError(String(error));
    } finally {
      setWorkspaceBusy(false);
    }
  }

  async function toggleWorkspaceArchived(workspace: Workspace) {
    setWorkspaceBusy(true);
    setWorkspaceError(null);
    try {
      await invoke<Workspace>("set_workspace_archived", {
        workspaceId: workspace.id,
        archived: workspace.archivedAt === null,
      });
      if (workspaceFilter === workspace.id) setWorkspaceFilter(null);
      await refreshWorkspaces(showArchivedWorkspaces);
    } catch (error) {
      setWorkspaceError(String(error));
    } finally {
      setWorkspaceBusy(false);
    }
  }

  async function deleteWorkspace(workspace: Workspace) {
    if (!window.confirm(`¿Eliminar el espacio «${workspace.name}»? Los proyectos se conservan; solo se borra la agrupación.`)) return;
    setWorkspaceBusy(true);
    setWorkspaceError(null);
    try {
      await invoke("delete_workspace", { workspaceId: workspace.id });
      if (workspaceFilter === workspace.id) setWorkspaceFilter(null);
      await refreshWorkspaces(showArchivedWorkspaces);
    } catch (error) {
      setWorkspaceError(String(error));
    } finally {
      setWorkspaceBusy(false);
    }
  }

  async function toggleProjectMembership(workspace: Workspace, projectId: number) {
    const member = workspace.projectIds.includes(projectId);
    setMembershipBusyKey(`${workspace.id}-${projectId}`);
    setWorkspaceError(null);
    try {
      await invoke<Workspace>(member ? "remove_workspace_project" : "add_workspace_project", {
        workspaceId: workspace.id,
        projectId,
      });
      await refreshWorkspaces(showArchivedWorkspaces);
    } catch (error) {
      setWorkspaceError(String(error));
    } finally {
      setMembershipBusyKey(null);
    }
  }

  const activeWorkspaces = useMemo(
    () => workspaces.filter((workspace) => workspace.archivedAt === null),
    [workspaces],
  );
  const visibleWorkspaces = useMemo(
    () => workspaces.filter((workspace) => showArchivedWorkspaces || workspace.archivedAt === null),
    [workspaces, showArchivedWorkspaces],
  );
  const selectedWorkspace = useMemo(
    () => typeof workspaceFilter === "number"
      ? workspaces.find((workspace) => workspace.id === workspaceFilter) ?? null
      : null,
    [workspaces, workspaceFilter],
  );
  const workspaceNamesForProject = useCallback(
    (projectId: number) => activeWorkspaces
      .filter((workspace) => workspace.projectIds.includes(projectId))
      .map((workspace) => workspace.name),
    [activeWorkspaces],
  );
  const visibleProjects = useMemo(
    () => projects.filter((project) => {
      if (!showArchivedProjects && project.archivedAt !== null) return false;
      if (workspaceFilter === "none") {
        return !activeWorkspaces.some((workspace) => workspace.projectIds.includes(project.id));
      }
      if (typeof workspaceFilter === "number") {
        return selectedWorkspace?.projectIds.includes(project.id) ?? false;
      }
      return true;
    }),
    [projects, showArchivedProjects, workspaceFilter, activeWorkspaces, selectedWorkspace],
  );

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

  useEffect(() => {
    if (connection.kind !== "connected") {
      setEventStreamStatus({ state: "stopped", retryInMs: null, message: null });
      setLastRealtimeSync(null);
      return;
    }

    let disposed = false;
    let statusUnlisten: (() => void) | undefined;
    let changeUnlisten: (() => void) | undefined;
    let reconciliationTimer: number | null = null;
    let reconciliationRunning = false;
    let reconciliationRequested = false;
    let needsFullReconciliation = true;

    const reconcile = async () => {
      if (disposed) return;
      if (reconciliationRunning) {
        reconciliationRequested = true;
        return;
      }

      reconciliationRunning = true;
      do {
        reconciliationRequested = false;
        const candidates = knownWorktreesRef.current.filter((worktree) =>
          worktree.opencodeSessionId
          && worktree.opencodeLocationMatches === true
          && (needsFullReconciliation || taskSnapshotsRef.current[worktree.id] !== undefined),
        );
        needsFullReconciliation = false;
        await reconcileWorktreeSnapshots(candidates, () => disposed);
      } while (reconciliationRequested && !disposed);

      reconciliationRunning = false;
      if (!disposed) setLastRealtimeSync(Date.now());
    };

    const scheduleReconciliation = () => {
      if (disposed || reconciliationTimer !== null) return;
      reconciliationTimer = window.setTimeout(() => {
        reconciliationTimer = null;
        void reconcile();
      }, 900);
    };

    const start = async () => {
      setEventStreamStatus({ state: "connecting", retryInMs: null, message: null });
      try {
        statusUnlisten = await listen<EventStreamStatus>("opencode-event-stream-status", (event) => {
          if (disposed) return;
          setEventStreamStatus(event.payload);
          if (event.payload.state === "connected") {
            needsFullReconciliation = true;
            scheduleReconciliation();
          }
        });
        changeUnlisten = await listen("opencode-state-changed", scheduleReconciliation);
        if (disposed) {
          statusUnlisten();
          changeUnlisten();
          return;
        }

        await invoke("start_opencode_event_stream", { baseUrl, username, password });
      } catch {
        if (!disposed) {
          setEventStreamStatus({
            state: "error",
            retryInMs: null,
            message: "No se pudo iniciar la suscripción a eventos de OpenCode.",
          });
        }
      }
    };

    void start();
    return () => {
      disposed = true;
      if (reconciliationTimer !== null) window.clearTimeout(reconciliationTimer);
      statusUnlisten?.();
      changeUnlisten?.();
      void invoke("stop_opencode_event_stream").catch(() => undefined);
    };
  }, [connection.kind, baseUrl, username, password]);

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
      if (connection.kind === "connected") {
        void reconcileWorktreeSnapshots(result.filter((worktree) => !taskSnapshotsRef.current[worktree.id]));
      }
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

  // El catálogo solo cambia al descubrirlo: se derivan las listas una vez por
  // catálogo en lugar de filtrarlas en cada render del panel.
  const selectableAgents = useMemo(
    () => catalogState.kind === "loaded"
      ? (catalogState.catalog.agents ?? []).filter((agent) => !agent.hidden && (agent.mode === "primary" || agent.mode === "all"))
      : [],
    [catalogState],
  );
  const selectableModels = useMemo(
    () => catalogState.kind === "loaded"
      ? (catalogState.catalog.models ?? []).filter((model) => model.enabled === true && model.providerId)
      : [],
    [catalogState],
  );

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

  async function reconcileWorktreeSnapshots(worktrees: Worktree[], isCancelled: () => boolean = () => false) {
    if (connection.kind !== "connected" || isCancelled()) return;
    const sessions = worktrees.filter((worktree) =>
      worktree.status === "ready"
      && worktree.opencodeSessionId
      && worktree.opencodeLocationMatches === true,
    );

    for (let index = 0; index < sessions.length; index += 4) {
      if (connection.kind !== "connected" || isCancelled()) return;
      const batch = sessions.slice(index, index + 4);
      await Promise.all(batch.map(async (worktree) => {
        try {
          const snapshot = await invoke<SessionSnapshot>("refresh_worktree_session", {
            baseUrl,
            username,
            password,
            worktreeId: worktree.id,
          });
          if (isCancelled()) return;
          setTaskSnapshots((current) => ({ ...current, [worktree.id]: snapshot }));
          setTaskSnapshotErrors((current) => ({ ...current, [worktree.id]: "" }));
        } catch (error) {
          if (!isCancelled()) {
            setTaskSnapshotErrors((current) => ({ ...current, [worktree.id]: String(error) }));
          }
        }
      }));
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
  const realtimeSyncTime = lastRealtimeSync
    ? new Date(lastRealtimeSync).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit", second: "2-digit" })
    : null;
  const realtimeStatusText = eventStreamStatus.state === "connected"
    ? realtimeSyncTime ? `En vivo · sincronizado ${realtimeSyncTime}` : "En vivo · sincronizando sesiones"
    : eventStreamStatus.state === "reconnecting"
      ? `Reconectando${eventStreamStatus.retryInMs ? ` en ${Math.ceil(eventStreamStatus.retryInMs / 1000)} s` : ""} · el estado puede estar desactualizado`
      : eventStreamStatus.state === "error"
        ? eventStreamStatus.message ?? "Eventos no disponibles · usa Actualizar estado"
        : eventStreamStatus.state === "connecting"
          ? "Conectando al flujo de eventos…"
          : "Eventos en pausa";

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
          <button className={`nav-item ${page === "usage" ? "active" : ""}`} type="button" onClick={() => setPage("usage")}>
            <span className="nav-icon usage-icon" aria-hidden="true" />
            <span>Uso</span>
            {page === "usage" && <span className="nav-indicator" />}
          </button>
          <button className={`nav-item ${page === "tasks" ? "active" : ""}`} type="button" onClick={() => setPage("tasks")}>
            <span className="nav-icon activity-icon" aria-hidden="true" />
            <span>Tareas</span>
            {page === "tasks" && <span className="nav-indicator" />}
          </button>
        </nav>

        <div className="sidebar-footer">
          <div className="local-indicator"><span /> Solo en este equipo</div>
          <div className="sidebar-version">PREVIEW · 0.1.0</div>
        </div>
      </aside>

      <main className="main-content">
        <header className="topbar">
          <div className="breadcrumb"><span>Stade Studio</span><i>/</i><strong>{page === "dashboard" ? "Resumen" : page === "usage" ? "Uso" : page === "tasks" ? "Tareas" : "Proyectos"}</strong></div>
          <div className="local-badge"><span className="local-badge-dot" /> DATOS LOCALES</div>
        </header>

        {page === "dashboard" && (
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
            {connection.kind === "connected" && (
              <div className={`event-stream-status stream-${eventStreamStatus.state}`} role="status" aria-live="polite" title={eventStreamStatus.message ?? undefined}>
                <span className="event-stream-dot" aria-hidden="true" />
                {realtimeStatusText}
              </div>
            )}
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
              <div className="roadmap-item done"><span className="roadmap-check">✓</span><span>Tareas, permisos y entregas</span></div>
              <div className="roadmap-item current"><span className="roadmap-pulse" /><span>Integración revisable de cambios</span><span className="roadmap-tag">SIGUIENTE</span></div>
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
        )}

        {page === "projects" && (
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
              <span>{projectsLoading ? "Cargando proyectos…" : `${visibleProjects.length} ${visibleProjects.length === 1 ? "proyecto visible" : "proyectos visibles"}`}</span>
              <div className="projects-toolbar-actions">
                <label className="workspace-filter">
                  <span>Espacio</span>
                  <select
                    value={workspaceFilter === null ? "" : workspaceFilter === "none" ? "none" : String(workspaceFilter)}
                    onChange={(event) => {
                      const value = event.currentTarget.value;
                      setWorkspaceFilter(value === "" ? null : value === "none" ? "none" : Number(value));
                    }}
                    disabled={workspacesLoading}
                  >
                    <option value="">Todos los espacios</option>
                    <option value="none">Sin espacio</option>
                    {visibleWorkspaces.map((workspace) => (
                      <option key={workspace.id} value={workspace.id}>
                        {workspace.name}{workspace.archivedAt ? " (archivado)" : ""}
                      </option>
                    ))}
                  </select>
                </label>
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
                <button className="secondary-button" type="button" onClick={() => { void refreshProjects(showArchivedProjects); void refreshWorkspaces(showArchivedWorkspaces); }} disabled={projectsLoading}>
                  Actualizar
                </button>
              </div>
            </div>

            <div className="workspaces-panel panel">
              <div className="workspaces-panel-header">
                <div>
                  <p className="eyebrow">AGRUPACIÓN LOCAL</p>
                  <h3>Espacios</h3>
                  <p>Agrupa proyectos que trabajan juntos (p. ej. base de datos, backend y frontend de una app). Un proyecto puede vivir en varios espacios a la vez.</p>
                </div>
                <label className="archived-toggle">
                  <input
                    type="checkbox"
                    checked={showArchivedWorkspaces}
                    onChange={(event) => {
                      const includeArchived = event.currentTarget.checked;
                      setShowArchivedWorkspaces(includeArchived);
                      void refreshWorkspaces(includeArchived);
                    }}
                  />
                  <span>Incluir archivados</span>
                </label>
              </div>
              {workspaceError && <p className="worktree-inline-error" role="alert">{workspaceError}</p>}
              <form className="workspace-create-row" onSubmit={createWorkspace}>
                <input
                  value={newWorkspaceName}
                  onChange={(event) => setNewWorkspaceName(event.currentTarget.value)}
                  placeholder="Nuevo espacio, p. ej. App finanzas"
                  maxLength={100}
                  aria-label="Nombre del nuevo espacio"
                />
                <button className="secondary-button" type="submit" disabled={workspaceBusy || !newWorkspaceName.trim()}>
                  {workspaceBusy ? "Guardando…" : "Crear espacio"}
                </button>
              </form>
              {workspacesLoading ? (
                <p className="worktree-list-empty">Cargando espacios…</p>
              ) : visibleWorkspaces.length === 0 ? (
                <p className="worktree-list-empty">Aún no hay espacios. Crea el primero para agrupar tus proyectos.</p>
              ) : (
                <div className="workspace-list">
                  {visibleWorkspaces.map((workspace) => (
                    <div className={`workspace-item ${workspace.archivedAt ? "workspace-archived" : ""}`} key={workspace.id}>
                      <button
                        className={`workspace-name-button ${workspaceFilter === workspace.id ? "active" : ""}`}
                        type="button"
                        onClick={() => setWorkspaceFilter((current) => current === workspace.id ? null : workspace.id)}
                        title="Filtrar proyectos por este espacio"
                      >
                        <strong>{workspace.name}</strong>
                        <span>{workspace.projectIds.length} {workspace.projectIds.length === 1 ? "proyecto" : "proyectos"}</span>
                      </button>
                      {editingWorkspaceId === workspace.id ? (
                        <form
                          className="workspace-rename-row"
                          onSubmit={(event) => { event.preventDefault(); void renameWorkspace(workspace); }}
                        >
                          <input
                            value={editingWorkspaceName}
                            onChange={(event) => setEditingWorkspaceName(event.currentTarget.value)}
                            maxLength={100}
                            aria-label="Nuevo nombre del espacio"
                          />
                          <button className="project-action-button" type="submit" disabled={workspaceBusy}>Guardar</button>
                          <button className="project-action-button" type="button" onClick={() => setEditingWorkspaceId(null)}>Cancelar</button>
                        </form>
                      ) : (
                        <div className="workspace-item-actions">
                          {!workspace.archivedAt && (
                            <button
                              className="project-action-button"
                              type="button"
                              onClick={() => { setEditingWorkspaceId(workspace.id); setEditingWorkspaceName(workspace.name); }}
                            >
                              Renombrar
                            </button>
                          )}
                          <button className="project-action-button" type="button" onClick={() => void toggleWorkspaceArchived(workspace)} disabled={workspaceBusy}>
                            {workspace.archivedAt ? "Restaurar" : "Archivar"}
                          </button>
                          <button className="project-action-button" type="button" onClick={() => void deleteWorkspace(workspace)} disabled={workspaceBusy}>
                            Eliminar
                          </button>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>

            {projectsError && <div className="feedback error-feedback project-list-error" role="alert"><span className="feedback-icon">!</span><div><strong>Error con proyectos</strong><span>{projectsError}</span></div></div>}

            {projectsLoading ? (
              <div className="project-empty-state">Cargando la lista local…</div>
            ) : visibleProjects.length === 0 ? (
              <div className="project-empty-state">
                <span className="empty-folder-icon" aria-hidden="true">▱</span>
                <h2>{workspaceFilter === null ? (showArchivedProjects ? "Todavía no hay proyectos" : "Aún no has añadido proyectos") : "Ningún proyecto en este espacio"}</h2>
                <p>{workspaceFilter === null
                  ? "Elige la carpeta raíz de un proyecto. La app solo registra la ruta; no copia ni modifica archivos."
                  : "Añade proyectos a este espacio desde el desplegable «Espacios» de cada tarjeta."}</p>
                {workspaceFilter === null && <button className="secondary-button" type="button" onClick={() => openProjectDialog()}>Añadir primer proyecto</button>}
              </div>
            ) : (
              <div className="project-list">
                {visibleProjects.map((project) => (
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
                      {workspaceNamesForProject(project.id).length > 0 && (
                        <div className="workspace-chips" aria-label="Espacios del proyecto">
                          {workspaceNamesForProject(project.id).map((name) => (
                            <button
                              key={name}
                              className="workspace-chip"
                              type="button"
                              onClick={() => {
                                const target = activeWorkspaces.find((workspace) => workspace.name === name);
                                if (target) setWorkspaceFilter(target.id);
                              }}
                              title="Filtrar por este espacio"
                            >
                              {name}
                            </button>
                          ))}
                        </div>
                      )}
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
                    {!project.archivedAt && (
                      <details className="workspace-membership">
                        <summary>Espacios ({workspaceNamesForProject(project.id).length})</summary>
                        {activeWorkspaces.length === 0 ? (
                          <p className="worktree-inline-note">Crea un espacio arriba para agrupar este proyecto.</p>
                        ) : (
                          <div className="workspace-membership-list">
                            {activeWorkspaces.map((workspace) => {
                              const member = workspace.projectIds.includes(project.id);
                              const busy = membershipBusyKey === `${workspace.id}-${project.id}`;
                              return (
                                <label className="workspace-membership-option" key={workspace.id}>
                                  <input
                                    type="checkbox"
                                    checked={member}
                                    disabled={busy}
                                    onChange={() => void toggleProjectMembership(workspace, project.id)}
                                  />
                                  <span>{workspace.name}{busy ? "…" : ""}</span>
                                </label>
                              );
                            })}
                          </div>
                        )}
                      </details>
                    )}
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

        {page === "usage" && (
          <UsagePage
            connected={connection.kind === "connected"}
            baseUrl={baseUrl}
            username={username}
            password={password}
            catalogModels={catalogState.kind === "loaded" ? catalogState.catalog.models : null}
            onOpenPanel={() => setPage("dashboard")}
          />
        )}

        {page === "tasks" && (
          <TasksPage
            baseUrl={baseUrl}
            username={username}
            password={password}
            connected={connection.kind === "connected"}
            projects={projects}
            workspaces={activeWorkspaces.map((workspace) => ({
              id: workspace.id,
              name: workspace.name,
              projectIds: workspace.projectIds,
            }))}
            agents={selectableAgents}
            models={selectableModels}
          />
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
