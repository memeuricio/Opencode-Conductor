import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface OpenCodeHealth {
  healthy: boolean;
  version: string;
}

type ConnectionState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "connected"; version: string }
  | { kind: "error"; message: string };

function App() {
  const [baseUrl, setBaseUrl] = useState("http://127.0.0.1:4096");
  const [username, setUsername] = useState("opencode");
  const [password, setPassword] = useState("");
  const [connection, setConnection] = useState<ConnectionState>({ kind: "idle" });

  async function checkConnection() {
    setConnection({ kind: "checking" });

    try {
      const health = await invoke<OpenCodeHealth>("check_opencode_connection", {
        baseUrl,
        username,
        password,
      });
      setConnection({ kind: "connected", version: health.version });
    } catch (error) {
      setConnection({ kind: "error", message: String(error) });
    }
  }

  const isChecking = connection.kind === "checking";

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
          <div>
            <p className="brand-name">OPENCODE</p>
            <p className="brand-caption">AGENT WORKSPACE</p>
          </div>
        </div>

        <div className="workspace-label">ESPACIO DE TRABAJO</div>
        <nav className="navigation" aria-label="Navegación principal">
          <button className="nav-item active" type="button">
            <span className="nav-icon grid-icon" aria-hidden="true" />
            <span>Panel</span>
            <span className="nav-indicator" />
          </button>
          <button className="nav-item" type="button" disabled>
            <span className="nav-icon folder-icon" aria-hidden="true" />
            <span>Proyectos</span>
            <span className="nav-soon">PRONTO</span>
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
          <div className="breadcrumb"><span>Workspace</span><i>/</i><strong>Resumen</strong></div>
          <div className="local-badge"><span className="local-badge-dot" /> EJECUCIÓN LOCAL</div>
        </header>

        <section className="page-heading">
          <div>
            <p className="eyebrow">TU CENTRO DE OPERACIONES</p>
            <h1>Coordina a tus agentes.</h1>
            <p className="page-description">
              Proyectos, modelos y sesiones de OpenCode en un solo lugar.
            </p>
          </div>
          <div className="heading-decoration" aria-hidden="true">
            <span className="orbit orbit-one" />
            <span className="orbit orbit-two" />
            <span className="orbit-core" />
          </div>
        </section>

        <section className="overview-grid" aria-label="Estado del espacio de trabajo">
          <article className="metric-card">
            <div className="metric-label"><span className="metric-dot purple" /> PROYECTOS</div>
            <div className="metric-value">—</div>
            <div className="metric-footnote">Se mostrarán aquí al añadirlos</div>
          </article>
          <article className="metric-card">
            <div className="metric-label"><span className="metric-dot blue" /> AGENTES ACTIVOS</div>
            <div className="metric-value">—</div>
            <div className="metric-footnote">Conecta OpenCode para descubrirlos</div>
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
                    onChange={(event) => setBaseUrl(event.currentTarget.value)}
                    placeholder="http://127.0.0.1:4096"
                    spellCheck={false}
                    autoComplete="url"
                  />
                </div>
                <button className="primary-button" type="submit" disabled={isChecking || !baseUrl.trim()}>
                  {isChecking ? <><span className="spinner" /> Comprobando</> : "Comprobar conexión"}
                </button>
              </div>

              <div className="credentials-row">
                <div className="credential-field username-field">
                  <label htmlFor="opencode-username">Usuario</label>
                  <input
                    id="opencode-username"
                    value={username}
                    onChange={(event) => setUsername(event.currentTarget.value)}
                    autoComplete="username"
                  />
                </div>
                <div className="credential-field password-field">
                  <label htmlFor="opencode-password">Contraseña del servidor</label>
                  <input
                    id="opencode-password"
                    type="password"
                    value={password}
                    onChange={(event) => setPassword(event.currentTarget.value)}
                    placeholder="Pega la contraseña temporal de OpenCode"
                    autoComplete="current-password"
                  />
                </div>
              </div>
              <p className="credential-note">Solo se mantiene en memoria mientras la aplicación está abierta.</p>
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

            <div className="panel-divider" />
            <div className="connection-hint">
              <span className="hint-icon">i</span>
              <span>En OpenCode v2, el servidor puede mostrar una contraseña temporal al iniciarse. La aplicación aún no inicia servidores por sí sola; esa es la siguiente integración.</span>
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
              <div className="roadmap-item current"><span className="roadmap-pulse" /><span>Conexión con OpenCode</span><span className="roadmap-tag">AHORA</span></div>
              <div className="roadmap-item"><span className="roadmap-number">03</span><span>Proyectos y agentes</span></div>
              <div className="roadmap-item"><span className="roadmap-number">04</span><span>Traspasos y coordinación</span></div>
            </div>
            <div className="privacy-note"><span className="lock-icon" aria-hidden="true">▣</span> Tus proyectos permanecen en este equipo.</div>
          </aside>
        </section>

        <footer className="page-footer">
          <span>OpenCode Conductor</span>
          <span>Hecho para coordinar, no para abrir más terminales.</span>
        </footer>
      </main>
    </div>
  );
}

export default App;
