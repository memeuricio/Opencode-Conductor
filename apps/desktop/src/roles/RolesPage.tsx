import { useCallback, useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import "../tasks/tasks.css";

export interface Role {
  id: number;
  name: string;
  description: string | null;
  instructions: string | null;
  agentId: string;
  providerId: string;
  modelId: string;
  fallbackProviderId: string | null;
  fallbackModelId: string | null;
  fileScope: string;
  matchKeywords: string;
  createdAt: string;
  updatedAt: string;
  archivedAt: string | null;
}

interface RoleAgentOption {
  id: string;
  name: string;
}

interface RoleModelOption {
  id: string;
  name: string;
  providerId: string | null;
}

interface RolesPageProps {
  agents: RoleAgentOption[];
  models: RoleModelOption[];
}

interface RoleFormState {
  name: string;
  description: string;
  instructions: string;
  agentId: string;
  modelIndex: string;
  fallbackModelIndex: string;
  fileScope: string;
  matchKeywords: string;
}

function emptyForm(agents: RoleAgentOption[]): RoleFormState {
  return {
    name: "",
    description: "",
    instructions: "",
    agentId: agents[0]?.id ?? "",
    modelIndex: "0",
    fallbackModelIndex: "",
    fileScope: "",
    matchKeywords: "",
  };
}

export default function RolesPage({ agents, models }: RolesPageProps) {
  const [roles, setRoles] = useState<Role[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [includeArchived, setIncludeArchived] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingRole, setEditingRole] = useState<Role | null>(null);
  const [form, setForm] = useState<RoleFormState>(() => emptyForm(agents));
  const [formError, setFormError] = useState<string | null>(null);
  const [formSaving, setFormSaving] = useState(false);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refreshRoles = useCallback(async (archived: boolean) => {
    setLoading(true);
    setError(null);
    try {
      setRoles(await invoke<Role[]>("list_roles", { includeArchived: archived }));
    } catch (loadError) {
      setError(String(loadError));
      setRoles([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshRoles(includeArchived);
  }, [includeArchived, refreshRoles]);

  function patchForm(patch: Partial<RoleFormState>) {
    setForm((current) => ({ ...current, ...patch }));
  }

  function openCreateDialog() {
    setEditingRole(null);
    setForm(emptyForm(agents));
    setFormError(null);
    setDialogOpen(true);
  }

  function openEditDialog(role: Role) {
    const modelIndex = models.findIndex(
      (model) => model.providerId === role.providerId && model.id === role.modelId,
    );
    const fallbackIndex = role.fallbackModelId
      ? models.findIndex(
        (model) => model.providerId === role.fallbackProviderId && model.id === role.fallbackModelId,
      )
      : -1;
    setEditingRole(role);
    setForm({
      name: role.name,
      description: role.description ?? "",
      instructions: role.instructions ?? "",
      agentId: role.agentId,
      modelIndex: modelIndex >= 0 ? String(modelIndex) : "0",
      fallbackModelIndex: fallbackIndex >= 0 ? String(fallbackIndex) : "",
      fileScope: role.fileScope,
      matchKeywords: role.matchKeywords,
    });
    setFormError(null);
    setDialogOpen(true);
  }

  async function saveRole(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const model = models[Number(form.modelIndex)];
    if (!model?.providerId || !form.agentId) {
      setFormError("Selecciona un perfil y un modelo preferido.");
      return;
    }
    const fallback = form.fallbackModelIndex === "" ? null : models[Number(form.fallbackModelIndex)];
    if (form.fallbackModelIndex !== "" && !fallback?.providerId) {
      setFormError("El modelo alternativo no es válido.");
      return;
    }
    setFormSaving(true);
    setFormError(null);
    try {
      const payload = {
        name: form.name,
        description: form.description.trim() === "" ? null : form.description,
        instructions: form.instructions.trim() === "" ? null : form.instructions,
        agentId: form.agentId,
        providerId: model.providerId,
        modelId: model.id,
        fallbackProviderId: fallback?.providerId ?? null,
        fallbackModelId: fallback?.id ?? null,
        fileScope: form.fileScope,
        matchKeywords: form.matchKeywords,
      };
      if (editingRole) {
        await invoke("update_role", { roleId: editingRole.id, ...payload });
        setNotice(`Rol «${form.name}» actualizado.`);
      } else {
        await invoke("create_role", payload);
        setNotice(`Rol «${form.name}» creado.`);
      }
      setDialogOpen(false);
      await refreshRoles(includeArchived);
    } catch (saveError) {
      setFormError(String(saveError));
    } finally {
      setFormSaving(false);
    }
  }

  async function toggleArchived(role: Role) {
    setBusyKey(`archive-${role.id}`);
    setError(null);
    try {
      await invoke("set_role_archived", { roleId: role.id, archived: role.archivedAt === null });
      await refreshRoles(includeArchived);
    } catch (archiveError) {
      setError(String(archiveError));
    } finally {
      setBusyKey(null);
    }
  }

  async function deleteRole(role: Role) {
    if (!window.confirm(`¿Eliminar el rol «${role.name}»? Las tareas existentes conservan sus valores; solo se pierde la plantilla.`)) return;
    setBusyKey(`delete-${role.id}`);
    setError(null);
    try {
      await invoke("delete_role", { roleId: role.id });
      setNotice(`Rol «${role.name}» eliminado.`);
      await refreshRoles(includeArchived);
    } catch (deleteError) {
      setError(String(deleteError));
    } finally {
      setBusyKey(null);
    }
  }

  const visibleRoles = (roles ?? []).filter((role) => includeArchived || role.archivedAt === null);

  return (
    <section className="tasks-page">
      <div className="page-heading tasks-page-heading">
        <div>
          <p className="eyebrow">ESPECIALIDADES REUTILIZABLES</p>
          <h1>Roles</h1>
          <p className="page-description">
            Define perfiles con instrucciones, modelo preferido y alternativo, ámbito de archivos y
            palabras clave. Al planificar, la app sugiere el rol que encaja y tú decides.
          </p>
        </div>
        <div className="tasks-heading-actions">
          <button className="primary-button" type="button" onClick={openCreateDialog}>
            <span aria-hidden="true">＋</span> Nuevo rol
          </button>
        </div>
      </div>

      <div className="tasks-toolbar panel">
        <div className="tasks-toolbar-row">
          <span className="tasks-count-label">
            {loading ? "Cargando roles…" : `${visibleRoles.length} ${visibleRoles.length === 1 ? "rol" : "roles"}`}
          </span>
          <div className="tasks-toolbar-buttons">
            <label className="archived-toggle">
              <input
                type="checkbox"
                checked={includeArchived}
                onChange={(event) => setIncludeArchived(event.currentTarget.checked)}
              />
              <span>Incluir archivados</span>
            </label>
            <button className="secondary-button" type="button" onClick={() => void refreshRoles(includeArchived)} disabled={loading}>
              {loading ? "Actualizando…" : "Actualizar"}
            </button>
          </div>
        </div>
      </div>

      {notice && <div className="feedback success-feedback tasks-feedback" role="status"><span className="feedback-icon">✓</span><div><span>{notice}</span></div></div>}
      {error && <div className="feedback error-feedback tasks-feedback" role="alert"><span className="feedback-icon">!</span><div><span>{error}</span></div></div>}

      {loading && roles === null ? (
        <div className="project-empty-state tasks-empty">Cargando roles…</div>
      ) : visibleRoles.length === 0 ? (
        <div className="project-empty-state tasks-empty">
          <span className="empty-folder-icon" aria-hidden="true">▱</span>
          <h2>Sin roles todavía</h2>
          <p>Crea roles como Backend, Frontend o Base de datos con su modelo y ámbito. Ejemplos: «api, endpoint» para backend, «migración, esquema» para base de datos.</p>
          <button className="secondary-button" type="button" onClick={openCreateDialog}>Crear primer rol</button>
        </div>
      ) : (
        <div className="task-list">
          {visibleRoles.map((role) => (
            <article className="task-card" key={role.id}>
              <div className="task-card-heading">
                <div className="task-card-title">
                  <h2>{role.name}</h2>
                  {role.archivedAt && <span className="task-status status-failed">ARCHIVADO</span>}
                </div>
                <div className="task-card-actions">
                  {!role.archivedAt && <button className="project-action-button" type="button" onClick={() => openEditDialog(role)}>Editar</button>}
                  <button
                    className="project-action-button"
                    type="button"
                    onClick={() => void toggleArchived(role)}
                    disabled={busyKey === `archive-${role.id}`}
                  >
                    {busyKey === `archive-${role.id}` ? "Guardando…" : role.archivedAt ? "Restaurar" : "Archivar"}
                  </button>
                  <button
                    className="project-action-button"
                    type="button"
                    onClick={() => void deleteRole(role)}
                    disabled={busyKey === `delete-${role.id}`}
                  >
                    {busyKey === `delete-${role.id}` ? "Eliminando…" : "Eliminar"}
                  </button>
                </div>
              </div>

              {role.description && <p className="task-objective">{role.description}</p>}
              {role.instructions && <p className="task-links subtle">Instrucciones: {role.instructions}</p>}

              <div className="task-meta-row">
                <span className="task-meta-chip">{role.agentId} · {role.providerId}/{role.modelId}</span>
                {role.fallbackModelId && role.fallbackProviderId && (
                  <span className="task-meta-chip">Alternativo: {role.fallbackProviderId}/{role.fallbackModelId}</span>
                )}
                {role.fileScope.trim() !== "" && (
                  <span className="task-meta-chip scope-chip" title={role.fileScope}>
                    Ámbito: {role.fileScope.split("\n")[0]}{role.fileScope.split("\n").length > 1 ? "…" : ""}
                  </span>
                )}
                {role.matchKeywords.trim() !== "" && (
                  <span className="task-meta-chip" title={role.matchKeywords}>Claves: {role.matchKeywords}</span>
                )}
              </div>
            </article>
          ))}
        </div>
      )}

      {dialogOpen && (
        <div className="project-modal-backdrop" role="presentation" onClick={() => !formSaving && setDialogOpen(false)}>
          <div className="project-modal task-modal" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <div className="project-modal-header">
              <div>
                <p className="eyebrow">{editingRole ? "EDITAR ROL" : "NUEVO ROL"}</p>
                <h2>{editingRole ? `Rol «${editingRole.name}»` : "Definir rol"}</h2>
              </div>
              <button className="modal-close-button" type="button" onClick={() => setDialogOpen(false)} disabled={formSaving}>✕</button>
            </div>
            <form className="project-form" onSubmit={saveRole}>
              <label htmlFor="role-name">Nombre</label>
              <input
                id="role-name"
                value={form.name}
                onChange={(event) => patchForm({ name: event.currentTarget.value })}
                placeholder="Backend"
                maxLength={100}
                required
              />
              <label htmlFor="role-description">Descripción <span>opcional</span></label>
              <input
                id="role-description"
                value={form.description}
                onChange={(event) => patchForm({ description: event.currentTarget.value })}
                placeholder="Construye la API sobre el contrato aprobado"
                maxLength={500}
              />
              <label htmlFor="role-instructions">Instrucciones <span>llegan al agente en cada tarea del rol</span></label>
              <textarea
                id="role-instructions"
                value={form.instructions}
                onChange={(event) => patchForm({ instructions: event.currentTarget.value })}
                placeholder="Sigue el contrato aprobado, cubre errores con pruebas…"
                rows={3}
                maxLength={4000}
              />
              <label htmlFor="role-agent">Perfil OpenCode</label>
              <select
                id="role-agent"
                value={form.agentId}
                onChange={(event) => patchForm({ agentId: event.currentTarget.value })}
                required
                disabled={agents.length === 0}
              >
                {agents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name} · {agent.id}</option>)}
              </select>
              <label htmlFor="role-model">Modelo preferido</label>
              <select
                id="role-model"
                value={form.modelIndex}
                onChange={(event) => patchForm({ modelIndex: event.currentTarget.value })}
                required
                disabled={models.length === 0}
              >
                {models.map((model, index) => (
                  <option key={`${model.providerId}/${model.id}`} value={String(index)}>
                    {model.name} · {model.providerId}/{model.id}
                  </option>
                ))}
              </select>
              <label htmlFor="role-fallback">Modelo alternativo <span>solo se usa si lo pides al lanzar</span></label>
              <select
                id="role-fallback"
                value={form.fallbackModelIndex}
                onChange={(event) => patchForm({ fallbackModelIndex: event.currentTarget.value })}
                disabled={models.length === 0}
              >
                <option value="">Ninguno</option>
                {models.map((model, index) => (
                  <option key={`${model.providerId}/${model.id}`} value={String(index)}>
                    {model.name} · {model.providerId}/{model.id}
                  </option>
                ))}
              </select>
              <label htmlFor="role-scope">Ámbito de archivos <span>opcional, una ruta o patrón por línea</span></label>
              <textarea
                id="role-scope"
                value={form.fileScope}
                onChange={(event) => patchForm({ fileScope: event.currentTarget.value })}
                placeholder={"src/api\nmigrations"}
                rows={2}
                maxLength={4000}
              />
              <label htmlFor="role-keywords">Palabras clave <span>separadas por comas, para sugerir el rol</span></label>
              <input
                id="role-keywords"
                value={form.matchKeywords}
                onChange={(event) => patchForm({ matchKeywords: event.currentTarget.value })}
                placeholder="api, endpoint, contrato"
                maxLength={500}
              />
              {agents.length === 0 && <p className="project-form-hint">Conecta OpenCode para elegir perfiles y modelos del catálogo.</p>}
              {formError && <div className="project-form-error" role="alert">{formError}</div>}
              <div className="project-form-actions">
                <button className="secondary-button" type="button" onClick={() => setDialogOpen(false)} disabled={formSaving}>Cancelar</button>
                <button className="primary-button" type="submit" disabled={formSaving || agents.length === 0 || models.length === 0}>
                  {formSaving ? "Guardando…" : editingRole ? "Guardar cambios" : "Crear rol"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </section>
  );
}
