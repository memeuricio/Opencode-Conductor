import { useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./agents.css";

interface Project {
  id: number;
  name: string;
}

interface AgentOption {
  id: string;
  name: string;
}

interface ModelOption {
  id: string;
  modelId: string | null;
  providerId: string | null;
  name: string;
}

interface ProjectAgent {
  projectId: number;
  role: string;
  agentId: string;
  providerId: string;
  modelId: string;
}

interface Props {
  project: Project;
  agents: AgentOption[];
  models: ModelOption[];
  onClose: () => void;
}

function modelKey(model: ModelOption) {
  return `${model.providerId ?? ""}/${model.id}`;
}

export default function ProjectBuilderDialog({ project, agents, models, onClose }: Props) {
  const [assignment, setAssignment] = useState<ProjectAgent | null>(null);
  const [agentId, setAgentId] = useState(agents[0]?.id ?? "");
  const [selectedModel, setSelectedModel] = useState(models[0] ? modelKey(models[0]) : "");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    void invoke<ProjectAgent[]>("list_project_agents", { projectId: project.id })
      .then((assignments) => {
        if (disposed) return;
        const current = assignments.find((entry) => entry.role === "builder") ?? null;
        setAssignment(current);
        if (!current) return;
        setAgentId(current.agentId);
        const model = models.find((entry) => entry.providerId === current.providerId && entry.id === current.modelId);
        if (model) setSelectedModel(modelKey(model));
      })
      .catch((loadError) => { if (!disposed) setError(String(loadError)); })
      .finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  }, [project.id, models]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const model = models.find((entry) => modelKey(entry) === selectedModel);
    if (!agentId || !model?.providerId) {
      setError("Conecta OpenCode y elige un perfil y modelo para el Builder.");
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await invoke<ProjectAgent>("save_project_agent", {
        projectId: project.id,
        agentId,
        providerId: model.providerId,
        modelId: model.id,
      });
      setAssignment(saved);
      setNotice(`Builder guardado para «${project.name}».`);
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="project-modal-backdrop" role="presentation" onClick={() => !busy && onClose()}>
      <section className="project-modal project-agents-modal" role="dialog" aria-modal="true" aria-labelledby="project-builder-title" onClick={(event) => event.stopPropagation()}>
        <div className="project-modal-header">
          <div>
            <p className="eyebrow">BUILDER DEL PROYECTO</p>
            <h2 id="project-builder-title">{project.name}</h2>
          </div>
          <button className="modal-close-button" type="button" onClick={onClose} disabled={busy} aria-label="Cerrar">×</button>
        </div>
        {loading ? <p className="project-form-hint">Cargando Builder…</p> : (
          <form className="project-form" onSubmit={(event) => void save(event)}>
            <label htmlFor="project-builder-agent">Perfil de OpenCode</label>
            <select id="project-builder-agent" value={agentId} onChange={(event) => setAgentId(event.currentTarget.value)} disabled={agents.length === 0 || busy}>
              {agents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
            </select>
            <label htmlFor="project-builder-model">Modelo</label>
            <select id="project-builder-model" value={selectedModel} onChange={(event) => setSelectedModel(event.currentTarget.value)} disabled={models.length === 0 || busy}>
              {models.map((model) => <option key={modelKey(model)} value={modelKey(model)}>{model.name} · {model.providerId}/{model.modelId ?? model.id}</option>)}
            </select>
            {assignment && <p className="project-form-hint">Las tareas de este proyecto se ejecutarán con este Builder.</p>}
            {agents.length === 0 || models.length === 0 ? <p className="project-form-hint">Conecta OpenCode para cargar perfiles y modelos.</p> : null}
            {error && <div className="project-form-error" role="alert">{error}</div>}
            {notice && <div className="feedback success-feedback" role="status"><span className="feedback-icon">✓</span><div><span>{notice}</span></div></div>}
            <div className="project-form-actions">
              <span>Las instrucciones base se editan en Perfiles.</span>
              <button className="primary-button" type="submit" disabled={busy || agents.length === 0 || models.length === 0}>
                {busy ? "Guardando…" : assignment ? "Guardar cambios" : "Asignar Builder"}
              </button>
            </div>
          </form>
        )}
      </section>
    </div>
  );
}
