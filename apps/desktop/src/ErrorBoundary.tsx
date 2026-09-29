import { Component, type ErrorInfo, type ReactNode } from "react";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  message: string | null;
}

/**
 * Evita que un error de render deje la ventana en blanco: muestra el fallo y
 * permite reintentar sin reiniciar la aplicación. No guarda ni envía el error.
 */
export default class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { message: null };

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    const message = error instanceof Error ? error.message : String(error);
    return { message };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Error de interfaz en Stade Studio:", error.message, info.componentStack);
  }

  render() {
    if (this.state.message === null) {
      return this.props.children;
    }

    return (
      <div className="project-modal-backdrop">
        <section className="project-modal" role="alert" aria-labelledby="error-boundary-title">
          <div className="project-modal-header">
            <div>
              <p className="eyebrow">ERROR DE INTERFAZ</p>
              <h2 id="error-boundary-title">Esta vista no se pudo dibujar</h2>
            </div>
          </div>
          <p className="project-form-hint">
            La aplicación sigue abierta y tus datos locales no se modificaron. Reintenta la vista; si el error
            se repite, cierra y vuelve a abrir Stade Studio.
          </p>
          <code className="error-boundary-detail">{this.state.message}</code>
          <div className="project-form-actions">
            <button
              className="primary-button"
              type="button"
              onClick={() => this.setState({ message: null })}
            >
              Reintentar
            </button>
          </div>
        </section>
      </div>
    );
  }
}
