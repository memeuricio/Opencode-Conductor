import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./usage.css";

interface CacheTotals {
  read: number;
  write: number;
}

interface TokenTotals {
  input: number;
  output: number;
  reasoning: number;
  cache: CacheTotals;
}

interface UsageTotals {
  cost: number;
  sessions: number;
  subagents: number;
  prompts: number;
  steps: number;
  tokens: TokenTotals;
  activeDays: number;
  streak: number;
}

interface UsageWindow {
  from: number;
  to: number;
  totals: UsageTotals;
}

interface UsageBucket {
  start: number;
  end: number;
  cost: number;
  steps: number;
}

interface ModelUsageSummary {
  providerId: string;
  modelId: string;
  name: string;
  cost: number;
  steps: number;
  tokens: TokenTotals;
}

interface ModelUsagePeriods {
  today: ModelUsageSummary[];
  fiveHour: ModelUsageSummary[];
  week: ModelUsageSummary[];
  month: ModelUsageSummary[];
}

interface SessionUsageSummary {
  id: string;
  title: string | null;
  agent: string | null;
  model: string | null;
  cost: number;
  tokens: number;
  updated: number | null;
}

interface UsageOverview {
  generatedAt: number;
  today: UsageWindow;
  fiveHour: UsageWindow;
  week: UsageWindow;
  month: UsageWindow;
  modelUsage: ModelUsagePeriods;
  hourly: UsageBucket[];
  daily: UsageBucket[];
  lifetime: UsageTotals;
  sessions: SessionUsageSummary[];
  warnings: string[];
}

interface CatalogModel {
  providerId: string | null;
  modelId: string | null;
  name: string;
}

export interface UsagePageProps {
  connected: boolean;
  baseUrl: string;
  username: string;
  password: string;
  catalogModels: CatalogModel[] | null;
  onOpenPanel: () => void;
}

const DAILY_DAY_OPTIONS = [7, 14, 30];
const MONTHS_SHORT = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
const MODEL_PERIODS = [
  { id: "today", label: "Hoy" },
  { id: "fiveHour", label: "5 h" },
  { id: "week", label: "7 días" },
  { id: "month", label: "Mes" },
] as const;

type ModelPeriod = (typeof MODEL_PERIODS)[number]["id"];
type ModelProviderFilter = "all" | "opencode-go";

function formatUsd(value: number): string {
  const digits = Math.abs(value) < 1 ? 4 : 2;
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: digits,
  }).format(value);
}

function formatTokens(value: number): string {
  if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(1)} mil M`;
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)} M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)} k`;
  return String(Math.round(value));
}

function formatCount(value: number): string {
  return new Intl.NumberFormat("es").format(value);
}

function formatHour(ms: number): string {
  return new Date(ms).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" });
}

function formatDay(ms: number): string {
  const date = new Date(ms);
  return `${date.getDate()} ${MONTHS_SHORT[date.getMonth()]}`;
}

function formatDateTime(ms: number | null): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString("es", {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function totalTokens(tokens: TokenTotals): number {
  return tokens.input + tokens.output + tokens.reasoning + tokens.cache.read + tokens.cache.write;
}

function sumBucketCost(buckets: UsageBucket[]): number {
  return buckets.reduce((sum, bucket) => sum + bucket.cost, 0);
}

function BucketChart({
  buckets,
  labelEvery,
  labelFor,
  emptyLabel,
}: {
  buckets: UsageBucket[];
  labelEvery: number;
  labelFor: (bucket: UsageBucket) => string;
  emptyLabel: string;
}) {
  const max = Math.max(0, ...buckets.map((bucket) => bucket.cost));
  if (buckets.length === 0) {
    return <p className="usage-chart-empty">{emptyLabel}</p>;
  }

  return (
    <div className="usage-bars">
      {buckets.map((bucket, index) => {
        const height = max > 0 && bucket.cost > 0 ? Math.max((bucket.cost / max) * 100, 4) : 0;
        const showLabel = buckets.length - 1 - index === 0 || index % labelEvery === 0;
        return (
          <div
            className="usage-bar-column"
            key={bucket.start}
            title={`${labelFor(bucket)} · ${formatUsd(bucket.cost)} · ${formatCount(bucket.steps)} pasos`}
          >
            <div className="usage-bar-track">
              <div className="usage-bar-fill" style={{ height: `${height}%` }} />
            </div>
            <span className="usage-bar-label">{showLabel ? labelFor(bucket) : ""}</span>
          </div>
        );
      })}
    </div>
  );
}

function TokenBreakdown({ tokens }: { tokens: TokenTotals }) {
  const parts = [
    { key: "input", label: "Entrada", value: tokens.input },
    { key: "output", label: "Salida", value: tokens.output },
    { key: "reasoning", label: "Razonamiento", value: tokens.reasoning },
    { key: "cache-read", label: "Caché leída", value: tokens.cache.read },
    { key: "cache-write", label: "Caché escrita", value: tokens.cache.write },
  ];
  const total = parts.reduce((sum, part) => sum + part.value, 0);

  if (total <= 0) {
    return <p className="usage-chart-empty">Sin tokens registrados en el mes en curso.</p>;
  }

  return (
    <>
      <div className="usage-token-strip">
        {parts.filter((part) => part.value > 0).map((part) => (
          <div
            key={part.key}
            className={`usage-token-segment token-${part.key}`}
            style={{ width: `${(part.value / total) * 100}%` }}
            title={`${part.label}: ${formatTokens(part.value)}`}
          />
        ))}
      </div>
      <div className="usage-token-legend">
        {parts.filter((part) => part.value > 0).map((part) => (
          <div className="usage-token-item" key={part.key}>
            <span className={`usage-token-dot token-${part.key}`} />
            <span className="usage-token-name">{part.label}</span>
            <span className="usage-token-value">{formatTokens(part.value)}</span>
          </div>
        ))}
      </div>
    </>
  );
}

export default function UsagePage({
  connected,
  baseUrl,
  username,
  password,
  catalogModels,
  onOpenPanel,
}: UsagePageProps) {
  const [days, setDays] = useState(14);
  const [modelPeriod, setModelPeriod] = useState<ModelPeriod>("today");
  const [modelProvider, setModelProvider] = useState<ModelProviderFilter>("all");
  const [overview, setOverview] = useState<UsageOverview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [consoleError, setConsoleError] = useState<string | null>(null);
  const requestId = useRef(0);

  const load = useCallback(async () => {
    if (!connected) return;
    const current = ++requestId.current;
    setLoading(true);
    setError(null);
    try {
      const now = new Date();
      const result = await invoke<UsageOverview>("get_opencode_usage", {
        baseUrl,
        username,
        password,
        utcOffsetMinutes: now.getTimezoneOffset(),
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
        localDayStartMs: new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime(),
        localMonthStartMs: new Date(now.getFullYear(), now.getMonth(), 1).getTime(),
        days,
      });
      if (requestId.current === current) {
        setOverview(result);
      }
    } catch (loadError) {
      if (requestId.current === current) {
        setError(String(loadError));
      }
    } finally {
      if (requestId.current === current) {
        setLoading(false);
      }
    }
  }, [baseUrl, username, password, days, connected]);

  useEffect(() => {
    if (connected) {
      void load();
    } else {
      requestId.current += 1;
      setOverview(null);
      setError(null);
      setLoading(false);
    }
  }, [connected, load]);

  const modelNames = new Map<string, string>();
  for (const model of catalogModels ?? []) {
    if (model.providerId && model.modelId) {
      modelNames.set(`${model.providerId}/${model.modelId}`.toLocaleLowerCase(), model.name);
    }
  }

  function modelLabel(model: string | null): string {
    if (!model) return "—";
    return modelNames.get(model.toLocaleLowerCase()) ?? model;
  }

  const monthTokens = overview ? totalTokens(overview.month.totals.tokens) : 0;
  const hourlyTotal = overview ? sumBucketCost(overview.hourly) : 0;
  const modelRows = overview?.modelUsage[modelPeriod] ?? [];
  const visibleModelRows = modelProvider === "all"
    ? modelRows
    : modelRows.filter((model) => model.providerId.toLocaleLowerCase() === "opencode-go");
  const modelWindow = overview ? overview[modelPeriod] : null;
  const modelChartTotal = modelProvider === "all"
    ? (modelWindow?.totals.cost ?? 0)
    : visibleModelRows.reduce((total, model) => total + model.cost, 0);

  async function openConsole() {
    setConsoleError(null);
    try {
      await openUrl("https://opencode.ai/console");
    } catch {
      setConsoleError("No se pudo abrir OpenCode Console en el navegador predeterminado.");
    }
  }

  return (
    <section className="usage-page">
      <div className="page-heading usage-page-heading">
        <div>
          <p className="eyebrow">ESTADÍSTICAS DEL SERVIDOR</p>
          <h1>Uso de OpenCode</h1>
          <p className="page-description">
            Gasto, modelos y tokens registrados por el servidor OpenCode conectado. La cuota de cuenta se consulta en Console.
          </p>
        </div>
        <div className="usage-heading-actions">
          <div className="usage-segmented" role="group" aria-label="Días del gráfico diario">
            {DAILY_DAY_OPTIONS.map((option) => (
              <button
                key={option}
                className={days === option ? "active" : ""}
                type="button"
                onClick={() => setDays(option)}
              >
                {option} días
              </button>
            ))}
          </div>
          <button className="secondary-button" type="button" onClick={() => void load()} disabled={!connected || loading}>
            {loading ? "Actualizando…" : "Actualizar"}
          </button>
        </div>
      </div>

      <article className="panel usage-account-panel">
        <div className="usage-account-copy">
          <p className="eyebrow">CUOTA GLOBAL DE CUENTA</p>
          <h2>OpenCode Go</h2>
          <p>
            La API del servidor conectado no expone el uso ni los reinicios oficiales de la cuenta. No mostramos un porcentaje
            local como si fuera la cuota global; Console es la fuente autoritativa e incluye actividad de otros equipos y clientes.
          </p>
        </div>
        <button className="secondary-button usage-console-link" type="button" onClick={() => void openConsole()}>
          Abrir OpenCode Console <span aria-hidden="true">↗</span>
        </button>
      </article>
      {consoleError && <p className="usage-console-error" role="status">{consoleError}</p>}

      {!connected && (
        <div className="project-empty-state usage-empty-state">
          <span className="empty-folder-icon" aria-hidden="true">◇</span>
          <h2>Conecta OpenCode para ver el uso</h2>
          <p>Conecta un servidor para consultar sus estadísticas locales. No se envían métricas a ningún servicio externo.</p>
          <button className="secondary-button" type="button" onClick={onOpenPanel}>
            Ir al panel para conectar
          </button>
        </div>
      )}

      {connected && error && (
        <div className="feedback error-feedback" role="alert">
          <span className="feedback-icon">!</span>
          <div>
            <strong>No se pudo cargar el uso</strong>
            <span>{error}</span>
          </div>
        </div>
      )}

      {connected && loading && !overview && !error && (
        <div className="project-empty-state usage-empty-state">
          <span className="spinner" aria-hidden="true" />
          <p>Consultando las estadísticas locales de OpenCode…</p>
        </div>
      )}

      {connected && overview && (
        <>
          <section className="usage-kpis" aria-label="Resumen de consumo">
            <article className="metric-card">
              <div className="metric-label"><span className="metric-dot purple" /> GASTO DE HOY</div>
              <div className="metric-value">{formatUsd(overview.today.totals.cost)}</div>
              <div className="metric-footnote">
                {formatCount(overview.today.totals.sessions)} sesiones · {formatCount(overview.today.totals.steps)} pasos
              </div>
            </article>
            <article className="metric-card">
              <div className="metric-label"><span className="metric-dot purple" /> MES CALENDARIO LOCAL</div>
              <div className="metric-value">{formatUsd(overview.month.totals.cost)}</div>
              <div className="metric-footnote">
                {formatCount(overview.month.totals.sessions)} sesiones · {formatCount(overview.month.totals.steps)} pasos
              </div>
            </article>
            <article className="metric-card">
              <div className="metric-label"><span className="metric-dot green" /> ÚLTIMAS 5 HORAS</div>
              <div className="metric-value">{formatUsd(overview.fiveHour.totals.cost)}</div>
              <div className="metric-footnote">Ventana móvil de 5 horas</div>
            </article>
            <article className="metric-card">
              <div className="metric-label"><span className="metric-dot blue" /> ÚLTIMOS 7 DÍAS</div>
              <div className="metric-value">{formatUsd(overview.week.totals.cost)}</div>
              <div className="metric-footnote">Ventana móvil semanal</div>
            </article>
            <article className="metric-card">
              <div className="metric-label"><span className="metric-dot purple" /> TOKENS DEL MES</div>
              <div className="metric-value">{formatTokens(monthTokens)}</div>
              <div className="metric-footnote">
                Entrada {formatTokens(overview.month.totals.tokens.input)} · Salida {formatTokens(overview.month.totals.tokens.output)}
              </div>
            </article>
            <article className="metric-card">
              <div className="metric-label"><span className="metric-dot green" /> HISTÓRICO</div>
              <div className="metric-value">{formatUsd(overview.lifetime.cost)}</div>
              <div className="metric-footnote">
                {formatCount(overview.lifetime.sessions)} sesiones · {formatCount(overview.lifetime.activeDays)} días activos
              </div>
            </article>
            <article className="metric-card">
              <div className="metric-label"><span className="metric-dot blue" /> RACHA ACTUAL</div>
              <div className="metric-value">{formatCount(overview.lifetime.streak)} días</div>
              <div className="metric-footnote">Días consecutivos con actividad</div>
            </article>
          </section>

          <section className="usage-charts" aria-label="Gráficos de consumo">
            <article className="panel usage-chart-card">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">ÚLTIMAS 24 HORAS</p>
                  <h2>Gasto por hora</h2>
                </div>
                <div className="usage-chart-total">{formatUsd(hourlyTotal)}</div>
              </div>
              <BucketChart
                buckets={overview.hourly}
                labelEvery={4}
                labelFor={(bucket) => formatHour(bucket.start)}
                emptyLabel="Sin actividad reciente."
              />
            </article>

            <article className="panel usage-chart-card">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">ÚLTIMOS {days} DÍAS</p>
                  <h2>Gasto por día</h2>
                </div>
                <div className="usage-chart-total">{formatUsd(sumBucketCost(overview.daily))}</div>
              </div>
              <BucketChart
                buckets={overview.daily}
                labelEvery={Math.max(1, Math.ceil(days / 7))}
                labelFor={(bucket) => formatDay(bucket.start)}
                emptyLabel="Sin actividad en el rango."
              />
            </article>

            <article className="panel usage-chart-card">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">SERVIDOR CONECTADO · COSTO REGISTRADO</p>
                  <h2>Uso por modelo</h2>
                </div>
                <div className="usage-chart-controls">
                  <div className="usage-segmented" role="group" aria-label="Período del uso por modelo">
                    {MODEL_PERIODS.map((period) => (
                      <button
                        key={period.id}
                        className={modelPeriod === period.id ? "active" : ""}
                        type="button"
                        onClick={() => setModelPeriod(period.id)}
                      >
                        {period.label}
                      </button>
                    ))}
                  </div>
                  <div className="usage-segmented" role="group" aria-label="Proveedor del uso por modelo">
                    <button className={modelProvider === "all" ? "active" : ""} type="button" onClick={() => setModelProvider("all")}>
                      Todos
                    </button>
                    <button className={modelProvider === "opencode-go" ? "active" : ""} type="button" onClick={() => setModelProvider("opencode-go")}>
                      Go
                    </button>
                  </div>
                  <div className="usage-chart-total">{formatUsd(modelChartTotal)}</div>
                </div>
              </div>
              <p className="usage-local-range">
                {MODEL_PERIODS.find((period) => period.id === modelPeriod)?.label} · zona horaria local ·{ " " }
                {modelProvider === "all" ? "todos los proveedores" : "proveedor OpenCode Go"}
              </p>
              {visibleModelRows.length === 0 ? (
                <p className="usage-chart-empty">Sin uso registrado para este filtro y período.</p>
              ) : (
                <div className="usage-model-bars">
                  {visibleModelRows.map((model) => {
                    const max = Math.max(...visibleModelRows.map((item) => item.cost), 0);
                    const width = max > 0 ? Math.max((model.cost / max) * 100, model.cost > 0 ? 3 : 0) : 0;
                    const share = modelChartTotal > 0 ? (model.cost / modelChartTotal) * 100 : 0;
                    const displayName = modelNames.get(`${model.providerId}/${model.modelId}`.toLocaleLowerCase()) ?? model.name;
                    return (
                      <div className="usage-model-row" key={`${model.providerId}/${model.modelId}`}>
                        <div className="usage-model-name">
                          <strong>{displayName}</strong>
                          <span>{model.providerId}/{model.modelId} · {formatCount(model.steps)} pasos · {formatTokens(totalTokens(model.tokens))} tokens</span>
                        </div>
                        <div className="usage-model-track" title={`${displayName}: ${formatUsd(model.cost)} (${share.toFixed(1)} %)`}>
                          <div className="usage-model-fill" style={{ width: `${width}%` }} />
                        </div>
                        <span className="usage-model-value">
                          {formatUsd(model.cost)} <small>{new Intl.NumberFormat("es", { maximumFractionDigits: 1 }).format(share)}%</small>
                        </span>
                      </div>
                    );
                  })}
                </div>
              )}
            </article>

            <article className="panel usage-chart-card">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">MES CALENDARIO LOCAL</p>
                  <h2>Distribución de tokens</h2>
                </div>
                <div className="usage-chart-total">{formatTokens(monthTokens)}</div>
              </div>
              <TokenBreakdown tokens={overview.month.totals.tokens} />
            </article>
          </section>

          <section className="usage-tables" aria-label="Sesiones recientes">
            <article className="panel usage-sessions-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">ÚLTIMAS SESIONES</p>
                  <h2>Gasto por sesión</h2>
                </div>
              </div>
              {overview.sessions.length === 0 ? (
                <p className="usage-chart-empty">Todavía no hay sesiones registradas.</p>
              ) : (
                <div className="usage-session-list">
                  {overview.sessions.map((session) => (
                    <div className="usage-session-row" key={session.id}>
                      <div className="usage-session-title">
                        <strong>{session.title ?? session.id}</strong>
                        <span>{modelLabel(session.model)}{session.agent ? ` · ${session.agent}` : ""}</span>
                      </div>
                      <div className="usage-session-meta">
                        <span className="usage-session-cost">{formatUsd(session.cost)}</span>
                        <span>{formatTokens(session.tokens)} tokens</span>
                        <span>{formatDateTime(session.updated)}</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </article>
          </section>

          {overview.warnings.length > 0 && (
            <div className="catalog-warnings usage-warnings" role="status">
              {overview.warnings.map((warning) => <p key={warning}>{warning}</p>)}
            </div>
          )}

          <p className="usage-disclaimer">
            Estas métricas describen las sesiones registradas por el servidor conectado; no son la cuota global de OpenCode Go ni
            incluyen otros servidores o clientes. Los períodos Hoy y Mes usan el calendario local; 5 h y 7 días son ventanas
            móviles. Actualizado {formatDateTime(overview.generatedAt)}.
          </p>
        </>
      )}
    </section>
  );
}
