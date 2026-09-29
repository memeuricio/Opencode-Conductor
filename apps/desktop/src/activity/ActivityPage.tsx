import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "../tasks/tasks.css";
import "./activity.css";

interface TimelineEntry {
  id: number;
  taskId: number;
  taskTitle: string;
  taskStatus: string;
  kind: string;
  detail: string | null;
  createdAt: string;
}

interface ActivityProjectOption {
  id: number;
  name: string;
  archivedAt: string | null;
}

interface ActivityPageProps {
  projects: ActivityProjectOption[];
}

const KIND_LABELS: Record<string, string> = {
  created: "Tarea creada",
  updated: "Definición actualizada",
  working: "Trabajo iniciado",
  dispatched: "Prompt enviado al agente",
  dispatch_failed: "Falló el envío del prompt",
  nudge_sent: "Entrega pedida al agente",
  fallback_applied: "Modelo alternativo aplicado",
  handoff_submitted: "Entrega registrada por el agente",
  handoff_accepted: "Entrega aceptada",
  handoff_returned: "Entrega devuelta con nota",
  blocker_reported: "Bloqueo reportado",
  decision_requested: "Pregunta al usuario",
  decision_answered: "Pregunta respondida",
  completed_manually: "Cierre manual",
  failed: "Marcada como fallida",
  reopened: "Tarea reabierta",
};

function kindLabel(kind: string) {
  return KIND_LABELS[kind] ?? kind;
}

function formatTime(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" });
}

function dayKey(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

function dayLabel(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "Hoy";
  if (date.toDateString() === yesterday.toDateString()) return "Ayer";
  return date.toLocaleDateString("es", { weekday: "long", day: "numeric", month: "long" });
}

export default function ActivityPage({ projects }: ActivityPageProps) {
  const activeProjects = useMemo(
    () => projects.filter((project) => project.archivedAt === null),
    [projects],
  );
  const [selectedProjectId, setSelectedProjectId] = useState<number | null>(null);
  const projectId = selectedProjectId ?? activeProjects[0]?.id ?? null;

  const [entries, setEntries] = useState<TimelineEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retention, setRetention] = useState<number | null>(null);
  const [retentionDraft, setRetentionDraft] = useState("30");
  const [retentionBusy, setRetentionBusy] = useState(false);
  const [retentionNotice, setRetentionNotice] = useState<string | null>(null);
  const [lastSync, setLastSync] = useState<number | null>(null);

  const refreshEntries = useCallback(async (project: number) => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<TimelineEntry[]>("list_project_activity", {
        projectId: project,
        limit: 200,
      });
      setEntries(result);
      setLastSync(Date.now());
    } catch (loadError) {
      setError(String(loadError));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    invoke<number>("get_activity_retention")
      .then((days) => {
        setRetention(days);
        setRetentionDraft(String(days));
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (projectId === null) {
      setEntries(null);
      return;
    }
    void refreshEntries(projectId);
  }, [projectId, refreshEntries]);

  useEffect(() => {
    let disposed = false;
    let unlistenCoordination: (() => void) | undefined;
    let unlistenStream: (() => void) | undefined;
    const refresh = () => {
      if (disposed || projectId === null) return;
      void refreshEntries(projectId);
    };
    void listen("coordination-changed", refresh).then((fn) => {
      unlistenCoordination = disposed ? (fn(), undefined) : fn;
    });
    void listen("opencode-state-changed", refresh).then((fn) => {
      unlistenStream = disposed ? (fn(), undefined) : fn;
    });
    return () => {
      disposed = true;
      unlistenCoordination?.();
      unlistenStream?.();
    };
  }, [projectId, refreshEntries]);

  async function saveRetention() {
    const days = Number(retentionDraft);
    setRetentionBusy(true);
    setRetentionNotice(null);
    setError(null);
    try {
      const saved = await invoke<number>("set_activity_retention", { days });
      setRetention(saved);
      setRetentionDraft(String(saved));
      setRetentionNotice(`Se conservan los últimos ${saved} días de actividad.`);
      if (projectId !== null) await refreshEntries(projectId);
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setRetentionBusy(false);
    }
  }

  const grouped = useMemo(() => {
    const groups: { key: string; label: string; items: TimelineEntry[] }[] = [];
    for (const entry of entries ?? []) {
      const key = dayKey(entry.createdAt);
      const existing = groups.find((group) => group.key === key);
      if (existing) {
        existing.items.push(entry);
      } else {
        groups.push({ key, label: dayLabel(entry.createdAt), items: [entry] });
      }
    }
    return groups;
  }, [entries]);

  return (
    <section className="tasks-page">
      <div className="page-heading tasks-page-heading">
        <div>
          <p className="eyebrow">LÍNEA DE TIEMPO LOCAL</p>
          <h1>Actividad</h1>
          <p className="page-description">
            Orquestación de cada proyecto en un solo lugar: tareas, entregas, preguntas y envíos.
            Se actualiza sola con los eventos en vivo; nunca guarda prompts ni respuestas.
          </p>
        </div>
      </div>

      <div className="tasks-toolbar panel">
        <div className="tasks-toolbar-row">
          <label className="tasks-project-select">
            <span>Proyecto</span>
            <select
              value={projectId ?? ""}
              onChange={(event) => {
                const value = event.currentTarget.value;
                setSelectedProjectId(value === "" ? null : Number(value));
              }}
              disabled={activeProjects.length === 0}
            >
              {activeProjects.length === 0 && <option value="">Sin proyectos activos</option>}
              {activeProjects.map((project) => (
                <option key={project.id} value={project.id}>{project.name}</option>
              ))}
            </select>
          </label>
          <div className="tasks-toolbar-buttons">
            <label className="retention-field">
              <span>Retención (días)</span>
              <input
                type="number"
                min={1}
                max={365}
                value={retentionDraft}
                onChange={(event) => setRetentionDraft(event.currentTarget.value)}
              />
            </label>
            <button className="secondary-button" type="button" onClick={() => void saveRetention()} disabled={retentionBusy}>
              {retentionBusy ? "Guardando…" : "Guardar"}
            </button>
            <button
              className="secondary-button"
              type="button"
              onClick={() => projectId !== null && void refreshEntries(projectId)}
              disabled={projectId === null || loading}
            >
              {loading ? "Actualizando…" : "Actualizar"}
            </button>
          </div>
        </div>
        <div className="tasks-bridge-row">
          <span className="timeline-live-dot" aria-hidden="true" />
          <span className="tasks-bridge-note">
            {lastSync
              ? `Sincronizado ${new Date(lastSync).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit", second: "2-digit" })} · se actualiza con los eventos en vivo`
              : "La línea de tiempo se actualiza con los eventos en vivo de OpenCode y de coordinación"}
            {retention !== null && ` · retención de ${retention} días`}
          </span>
        </div>
      </div>

      {retentionNotice && <div className="feedback success-feedback tasks-feedback" role="status"><span className="feedback-icon">✓</span><div><span>{retentionNotice}</span></div></div>}
      {error && <div className="feedback error-feedback tasks-feedback" role="alert"><span className="feedback-icon">!</span><div><span>{error}</span></div></div>}

      {projectId === null ? (
        <div className="project-empty-state tasks-empty">
          <span className="empty-folder-icon" aria-hidden="true">▱</span>
          <h2>Sin proyectos activos</h2>
          <p>Registra un proyecto para ver su actividad.</p>
        </div>
      ) : loading && entries === null ? (
        <div className="project-empty-state tasks-empty">Cargando la línea de tiempo…</div>
      ) : (entries ?? []).length === 0 ? (
        <div className="project-empty-state tasks-empty">
          <span className="empty-folder-icon" aria-hidden="true">▱</span>
          <h2>Sin actividad todavía</h2>
          <p>Crea y lanza tareas en este proyecto; cada paso quedará registrado aquí.</p>
        </div>
      ) : (
        <div className="timeline">
          {grouped.map((group) => (
            <div className="timeline-day" key={group.key}>
              <p className="timeline-day-label">{group.label}</p>
              {group.items.map((entry) => (
                <div className="timeline-item" key={entry.id}>
                  <span className="timeline-time">{formatTime(entry.createdAt)}</span>
                  <span className="timeline-kind">{kindLabel(entry.kind)}</span>
                  <div className="timeline-body">
                    <p className="timeline-task">#{entry.taskId} · {entry.taskTitle}</p>
                    {entry.detail && <p className="timeline-detail">{entry.detail}</p>}
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
