import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./agents.css";

type AgentRole = "builder";

interface AgentProfile {
  role: AgentRole;
  name: string;
  instructions: string;
  updatedAt: string;
}

const PROFILE_ORDER: AgentRole[] = ["builder"];

export default function AgentProfilesPage() {
  const [profiles, setProfiles] = useState<AgentProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyRole, setBusyRole] = useState<AgentRole | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    void invoke<AgentProfile[]>("list_agent_profiles")
      .then((result) => { if (!disposed) setProfiles(result); })
      .catch((loadError) => { if (!disposed) setError(String(loadError)); })
      .finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  }, []);

  function updateInstructions(role: AgentRole, instructions: string) {
    setProfiles((current) => current.map((profile) => profile.role === role ? { ...profile, instructions } : profile));
  }

  async function save(profile: AgentProfile) {
    setBusyRole(profile.role);
    setError(null);
    setNotice(null);
    try {
      const updated = await invoke<AgentProfile>("update_agent_profile", {
        role: profile.role,
        instructions: profile.instructions,
      });
      setProfiles((current) => current.map((entry) => entry.role === updated.role ? updated : entry));
      setNotice(`Instrucciones base del ${updated.name} guardadas.`);
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setBusyRole(null);
    }
  }

  return (
    <section className="agent-profiles-page">
      <div className="page-heading">
        <div>
          <p className="eyebrow">INSTRUCCIONES REUTILIZABLES</p>
          <h1>Perfiles de agentes</h1>
          <p className="page-description">Define las instrucciones base que recibe cada Builder.</p>
        </div>
      </div>

      {notice && <div className="feedback success-feedback" role="status"><span className="feedback-icon">✓</span><div><span>{notice}</span></div></div>}
      {error && <div className="feedback error-feedback" role="alert"><span className="feedback-icon">!</span><div><span>{error}</span></div></div>}
      {loading ? <div className="project-empty-state">Cargando perfiles…</div> : (
        <div className="agent-profile-list">
          {PROFILE_ORDER.map((role) => {
            const profile = profiles.find((entry) => entry.role === role);
            if (!profile) return null;
            return (
              <article className="panel agent-profile-card" key={role}>
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">IMPLEMENTA LAS TAREAS</p>
                    <h2>{profile.name}</h2>
                  </div>
                </div>
                <label htmlFor={`profile-${role}`}>Instrucciones base <span>opcional</span></label>
                <textarea
                  id={`profile-${role}`}
                  value={profile.instructions}
                  onChange={(event) => updateInstructions(role, event.currentTarget.value)}
                  rows={7}
                  maxLength={12_000}
                  placeholder="Implementa solo la tarea asignada y registra siempre la entrega en Stade Studio."
                />
                <div className="agent-profile-actions">
                  <span>Se aplican a todos los proyectos que usen este agente.</span>
                  <button className="primary-button" type="button" onClick={() => void save(profile)} disabled={busyRole !== null}>
                    {busyRole === role ? "Guardando…" : "Guardar instrucciones"}
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
