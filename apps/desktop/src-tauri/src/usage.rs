use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::opencode;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const STATS_TIMEOUT: Duration = Duration::from_secs(20);
const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;
const FIVE_HOURS_MS: i64 = 5 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;
const HOURLY_BUCKETS: i64 = 24;
const DEFAULT_DAILY_BUCKETS: i64 = 14;
const MIN_DAILY_BUCKETS: i64 = 7;
const MAX_DAILY_BUCKETS: i64 = 31;
const MAX_BATCH_CONCURRENCY: usize = 8;
const MAX_TOOLS: usize = 12;
const MAX_SESSIONS: usize = 12;
const GO_PROVIDER_ID: &str = "opencode-go";

/// Referencia de límites publicada por OpenCode Go. Se contrasta con
/// `https://opencode.ai/v2/docs/console/go/`; el plan mensual define el límite
/// por modelo y las ventanas usan 20 % (5 h), 50 % (semanal) y 100 % (mensual).
#[derive(Clone, Copy)]
enum LimitReference {
    Limited { go: f64, go_plus: f64 },
    Unlimited,
}

const GO_MODEL_LIMITS: &[(&str, &str, LimitReference)] = &[
    (
        "glm-5.3-flash",
        "GLM-5.3-Flash",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 180.0,
        },
    ),
    (
        "glm-5.3",
        "GLM-5.3",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 120.0,
        },
    ),
    (
        "glm-5.2",
        "GLM-5.2",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 180.0,
        },
    ),
    (
        "kimi-k3",
        "Kimi K3",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "kimi-k2.7-code",
        "Kimi K2.7 Code",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 180.0,
        },
    ),
    (
        "kimi-k2.6",
        "Kimi K2.6",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 240.0,
        },
    ),
    (
        "longcat-2.0",
        "LongCat-2.0",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 240.0,
        },
    ),
    (
        "longcat-2.5-preview-free",
        "LongCat 2.5 Preview Free",
        LimitReference::Unlimited,
    ),
    (
        "mimo-v2.6-flash",
        "MiMo-V2.6-Flash",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 120.0,
        },
    ),
    (
        "mimo-v2.6-pro",
        "MiMo-V2.6-Pro",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "mimo-v2.5",
        "MiMo-V2.5",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 120.0,
        },
    ),
    (
        "mimo-v2.5-pro",
        "MiMo-V2.5-Pro",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "minimax-m3",
        "MiniMax-M3",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 180.0,
        },
    ),
    (
        "minimax-m2.7",
        "MiniMax-M2.7",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 240.0,
        },
    ),
    (
        "muse-spark-1.3-contributor",
        "Muse Spark 1.3 Contributor",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 120.0,
        },
    ),
    (
        "muse-spark-1.2-contributor",
        "Muse Spark 1.2 Contributor",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 120.0,
        },
    ),
    (
        "qwen3.8-max",
        "Qwen3.8 Max",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "qwen3.8-flash",
        "Qwen3.8 Flash",
        LimitReference::Limited {
            go: 30.0,
            go_plus: 90.0,
        },
    ),
    (
        "qwen3.7-plus",
        "Qwen3.7 Plus",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 180.0,
        },
    ),
    (
        "deepseek-v4.1-flash",
        "DeepSeek V4.1 Flash",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 120.0,
        },
    ),
    (
        "deepseek-v4-pro",
        "DeepSeek V4 Pro",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "deepseek-v4-flash",
        "DeepSeek V4 Flash",
        LimitReference::Limited {
            go: 30.0,
            go_plus: 120.0,
        },
    ),
    (
        "deepseek-v4-flash-vision-exp",
        "DeepSeek V4 Flash Vision Exp",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "hy4-preview",
        "Hy4 preview",
        LimitReference::Limited {
            go: 30.0,
            go_plus: 120.0,
        },
    ),
    (
        "hy3",
        "Hy3",
        LimitReference::Limited {
            go: 60.0,
            go_plus: 240.0,
        },
    ),
    (
        "space-bunny-free",
        "Space Bunny Free",
        LimitReference::Unlimited,
    ),
    (
        "grok-4.7",
        "Grok 4.7",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "grok-4.6",
        "Grok 4.6",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "gpt-6-luna",
        "GPT 6 Luna",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
    (
        "gpt-5.6-luna",
        "GPT 5.6 Luna",
        LimitReference::Limited {
            go: 15.0,
            go_plus: 60.0,
        },
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum UsagePlan {
    Go,
    GoPlus,
}

impl UsagePlan {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "go" => Ok(Self::Go),
            "go_plus" | "go-plus" | "goplus" | "go plus" => Ok(Self::GoPlus),
            _ => Err("El plan de OpenCode Go indicado no es válido".to_string()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Go => "go",
            Self::GoPlus => "go_plus",
        }
    }

    fn reference_limit(self, reference: LimitReference) -> Option<f64> {
        match reference {
            LimitReference::Limited { go, go_plus } => Some(match self {
                Self::Go => go,
                Self::GoPlus => go_plus,
            }),
            LimitReference::Unlimited => None,
        }
    }
}

/// Ratio de la ventana sobre el límite mensual: 20 % a 5 horas, 50 % semanal
/// y 100 % mensual, según la documentación del plan.
const FIVE_HOUR_FRACTION: f64 = 0.2;
const WEEK_FRACTION: f64 = 0.5;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TokenTotals {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub reasoning: f64,
    #[serde(default)]
    pub cache: CacheTotals,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CacheTotals {
    #[serde(default)]
    pub read: f64,
    #[serde(default)]
    pub write: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiModelRef {
    id: String,
    #[serde(rename = "providerID")]
    provider_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiModelUsage {
    model: ApiModelRef,
    #[serde(default)]
    steps: u64,
    #[serde(default)]
    tokens: TokenTotals,
    #[serde(default)]
    cost: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiToolUsage {
    name: String,
    #[serde(default)]
    calls: u64,
    #[serde(default)]
    succeeded: u64,
    #[serde(default)]
    failed: u64,
    #[serde(default)]
    unfinished: u64,
    #[serde(default)]
    duration_p50: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiTools {
    #[serde(default)]
    usage: Option<Vec<ApiToolUsage>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsData {
    #[serde(default)]
    sessions: u64,
    #[serde(default)]
    subagents: u64,
    #[serde(default)]
    prompts: u64,
    #[serde(default)]
    steps: u64,
    #[serde(default)]
    tokens: TokenTotals,
    #[serde(default)]
    cost: f64,
    #[serde(default)]
    tools: Option<ApiTools>,
    #[serde(default)]
    active_days: u64,
    #[serde(default)]
    streak: u64,
    #[serde(default)]
    models: Vec<ApiModelUsage>,
}

#[derive(Debug, Deserialize)]
struct StatsEnvelope {
    data: StatsData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiSessionTime {
    #[serde(default)]
    updated: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiSession {
    id: String,
    #[serde(rename = "parentID", default)]
    parent_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    model: Option<ApiModelRef>,
    #[serde(default)]
    cost: f64,
    #[serde(default)]
    tokens: Option<TokenTotals>,
    #[serde(default)]
    time: Option<ApiSessionTime>,
}

#[derive(Debug, Deserialize)]
struct SessionsEnvelope {
    data: Vec<ApiSession>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub cost: f64,
    pub sessions: u64,
    pub subagents: u64,
    pub prompts: u64,
    pub steps: u64,
    pub tokens: TokenTotals,
    pub active_days: u64,
    pub streak: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub from: i64,
    pub to: i64,
    pub totals: UsageTotals,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageBucket {
    pub start: i64,
    pub end: i64,
    pub cost: f64,
    pub steps: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSlice {
    pub spent: f64,
    pub limit: Option<f64>,
    pub ratio: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitStatus {
    Limited,
    Unlimited,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoModelProgress {
    pub provider_id: String,
    pub model_id: String,
    pub name: String,
    pub limit_status: LimitStatus,
    pub monthly_limit: Option<f64>,
    pub five_hour: ProgressSlice,
    pub week: ProgressSlice,
    pub month: ProgressSlice,
    pub steps: u64,
    pub tokens: TokenTotals,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageSummary {
    pub provider_id: String,
    pub model_id: String,
    pub name: String,
    pub cost: f64,
    pub steps: u64,
    pub tokens: TokenTotals,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsagePeriods {
    pub today: Vec<ModelUsageSummary>,
    pub five_hour: Vec<ModelUsageSummary>,
    pub week: Vec<ModelUsageSummary>,
    pub month: Vec<ModelUsageSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUsageSummary {
    pub name: String,
    pub calls: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub unfinished: u64,
    pub duration_p50: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageSummary {
    pub id: String,
    pub title: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub cost: f64,
    pub tokens: f64,
    pub updated: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowAggregate {
    pub from: i64,
    pub to: i64,
    pub spent: f64,
    pub allowance: Option<f64>,
    pub ratio: Option<f64>,
    pub model_name: Option<String>,
    pub model_id: Option<String>,
    pub limit_status: LimitStatus,
    pub resets_at: Option<i64>,
    pub reset_is_estimate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageAggregates {
    pub rolling: WindowAggregate,
    pub weekly: WindowAggregate,
    pub monthly: WindowAggregate,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageOverview {
    pub generated_at: i64,
    pub plan: String,
    pub today: UsageWindow,
    pub five_hour: UsageWindow,
    pub week: UsageWindow,
    pub month: UsageWindow,
    pub model_usage: ModelUsagePeriods,
    pub aggregates: UsageAggregates,
    pub hourly: Vec<UsageBucket>,
    pub daily: Vec<UsageBucket>,
    pub lifetime: UsageTotals,
    pub go_limits: Vec<GoModelProgress>,
    pub top_models: Vec<ModelUsageSummary>,
    pub tools: Vec<ToolUsageSummary>,
    pub sessions: Vec<SessionUsageSummary>,
    pub warnings: Vec<String>,
}

fn now_millis() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .map_err(|_| "El reloj del sistema no es válido".to_string())
}

fn usage_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(STATS_TIMEOUT)
        .build()
        .map_err(|_| "No se pudo preparar la conexión con OpenCode".to_string())
}

fn stats_url(base_url: &Url) -> Result<Url, String> {
    let mut url = base_url.clone();
    url.set_path("/api/experimental/session/stats");
    Ok(url)
}

fn sessions_url(base_url: &Url) -> Result<Url, String> {
    let mut url = base_url.clone();
    url.set_path("/api/session");
    url.query_pairs_mut()
        .append_pair("limit", "20")
        .append_pair("order", "desc");
    Ok(url)
}

fn map_stats_request_error(error: reqwest::Error) -> String {
    if error.is_connect() {
        "No hay un servidor OpenCode escuchando en esa dirección".to_string()
    } else if error.is_timeout() {
        "OpenCode tardó demasiado en responder a las estadísticas de uso".to_string()
    } else {
        "No se pudo consultar el uso a OpenCode".to_string()
    }
}

fn stats_response_error(status: reqwest::StatusCode) -> String {
    match status {
        reqwest::StatusCode::UNAUTHORIZED => {
            "OpenCode rechazó las credenciales al consultar el uso".to_string()
        }
        reqwest::StatusCode::NOT_FOUND => {
            "Esta versión de OpenCode no expone estadísticas de uso".to_string()
        }
        _ => format!(
            "OpenCode respondió con un error HTTP ({}) al consultar el uso",
            status.as_u16()
        ),
    }
}

async fn fetch_stats(
    client: &reqwest::Client,
    base_url: &Url,
    username: &str,
    password: &str,
    range: Option<(i64, i64)>,
    tools: &str,
    timezone: &str,
) -> Result<StatsData, String> {
    // `from` y `to` son milisegundos epoch; el servidor devuelve HTTP 500 con
    // fechas ISO. El endpoint es experimental, así que un 404 se reporta como
    // versión sin estadísticas en lugar de intentar una ruta alternativa.
    let mut url = stats_url(base_url)?;
    {
        let mut query = url.query_pairs_mut();
        if let Some((from, to)) = range {
            query.append_pair("from", &from.to_string());
            query.append_pair("to", &to.to_string());
        }
        query.append_pair("tools", tools);
        query.append_pair("timezone", timezone);
    }

    let response = client
        .get(url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(map_stats_request_error)?;

    let status = response.status();
    if !status.is_success() {
        return Err(stats_response_error(status));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type.contains("application/json") {
        return Err(
            "OpenCode no devolvió las estadísticas como JSON; comprueba la versión instalada"
                .to_string(),
        );
    }

    response
        .json::<StatsEnvelope>()
        .await
        .map(|envelope| envelope.data)
        .map_err(|_| "OpenCode devolvió un formato de estadísticas no compatible".to_string())
}

async fn fetch_recent_sessions(
    client: &reqwest::Client,
    base_url: &Url,
    username: &str,
    password: &str,
) -> Result<Vec<SessionUsageSummary>, String> {
    let response = client
        .get(sessions_url(base_url)?)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(map_stats_request_error)?;

    let status = response.status();
    if !status.is_success() {
        return Err(stats_response_error(status));
    }

    let envelope: SessionsEnvelope = response
        .json()
        .await
        .map_err(|_| "OpenCode devolvió un formato de sesiones no compatible".to_string())?;

    let mut sessions: Vec<SessionUsageSummary> = envelope
        .data
        .into_iter()
        .filter(|session| session.parent_id.is_none())
        .map(|session| SessionUsageSummary {
            id: session.id,
            title: session.title.filter(|title| !title.trim().is_empty()),
            agent: session.agent,
            model: session
                .model
                .map(|model| format!("{}/{}", model.provider_id, model.id)),
            cost: session.cost,
            tokens: session
                .tokens
                .map(|tokens| {
                    tokens.input
                        + tokens.output
                        + tokens.reasoning
                        + tokens.cache.read
                        + tokens.cache.write
                })
                .unwrap_or(0.0),
            updated: session.time.and_then(|time| time.updated),
        })
        .collect();

    sessions.sort_by(|left, right| right.updated.unwrap_or(0).cmp(&left.updated.unwrap_or(0)));
    sessions.truncate(MAX_SESSIONS);

    Ok(sessions)
}

/// Ejecuta las consultas de intervalos en lotes con concurrencia acotada,
/// conservando el orden solicitado para poder etiquetar cada bucket.
async fn fetch_stats_batch(
    client: &reqwest::Client,
    base_url: &Url,
    username: &str,
    password: &str,
    ranges: &[(i64, i64)],
    timezone: &str,
) -> Vec<Result<StatsData, String>> {
    let mut results: Vec<Result<StatsData, String>> = Vec::with_capacity(ranges.len());

    for chunk in ranges.chunks(MAX_BATCH_CONCURRENCY) {
        let mut slots: Vec<Option<Result<StatsData, String>>> =
            (0..chunk.len()).map(|_| None).collect();
        let mut joined_set = tokio::task::JoinSet::new();

        for (index, (from, to)) in chunk.iter().enumerate() {
            let client = client.clone();
            let url = base_url.clone();
            let username = username.to_string();
            let password = password.to_string();
            let timezone = timezone.to_string();
            let from = *from;
            let to = *to;
            joined_set.spawn(async move {
                let result = fetch_stats(
                    &client,
                    &url,
                    &username,
                    &password,
                    Some((from, to)),
                    "none",
                    &timezone,
                )
                .await;
                (index, result)
            });
        }

        while let Some(joined) = joined_set.join_next().await {
            match joined {
                Ok((index, result)) => slots[index] = Some(result),
                Err(_) => {}
            }
        }

        for slot in slots {
            results.push(slot.unwrap_or_else(|| {
                Err("No se pudo completar una consulta de estadísticas".to_string())
            }));
        }
    }

    results
}

fn buckets_from_stats(
    ranges: &[(i64, i64)],
    results: Vec<Result<StatsData, String>>,
    warnings: &mut Vec<String>,
    label: &str,
) -> Vec<UsageBucket> {
    let failed = results.iter().filter(|result| result.is_err()).count();
    if failed > 0 {
        warnings.push(format!(
            "No se pudieron cargar {failed} intervalos de {label}; se muestran en cero"
        ));
    }

    ranges
        .iter()
        .zip(results)
        .map(|((start, end), result)| match result {
            Ok(stats) => UsageBucket {
                start: *start,
                end: *end,
                cost: stats.cost,
                steps: stats.steps,
            },
            Err(_) => UsageBucket {
                start: *start,
                end: *end,
                cost: 0.0,
                steps: 0,
            },
        })
        .collect()
}

fn totals_from_stats(stats: &StatsData) -> UsageTotals {
    UsageTotals {
        cost: stats.cost,
        sessions: stats.sessions,
        subagents: stats.subagents,
        prompts: stats.prompts,
        steps: stats.steps,
        tokens: stats.tokens.clone(),
        active_days: stats.active_days,
        streak: stats.streak,
    }
}

fn model_display_name(provider_id: &str, model_id: &str) -> String {
    if provider_id.eq_ignore_ascii_case(GO_PROVIDER_ID) {
        if let Some((_, name, _)) = GO_MODEL_LIMITS
            .iter()
            .find(|(id, _, _)| id.eq_ignore_ascii_case(model_id))
        {
            return (*name).to_string();
        }
    }
    model_id.to_string()
}

/// OpenCode puede devolver varias entradas para el mismo modelo con distinta
/// capitalización. Consolídalas antes de presentarlas o calcular límites para
/// evitar filas repetidas y porcentajes subestimados.
fn consolidated_model_usage(models: &[ApiModelUsage]) -> Vec<ApiModelUsage> {
    let mut consolidated: HashMap<(String, String), ApiModelUsage> = HashMap::new();

    for entry in models {
        let key = (
            entry.model.provider_id.trim().to_ascii_lowercase(),
            entry.model.id.trim().to_ascii_lowercase(),
        );
        if let Some(existing) = consolidated.get_mut(&key) {
            existing.steps = existing.steps.saturating_add(entry.steps);
            existing.cost += entry.cost;
            existing.tokens.input += entry.tokens.input;
            existing.tokens.output += entry.tokens.output;
            existing.tokens.reasoning += entry.tokens.reasoning;
            existing.tokens.cache.read += entry.tokens.cache.read;
            existing.tokens.cache.write += entry.tokens.cache.write;
        } else {
            consolidated.insert(key, entry.clone());
        }
    }

    consolidated.into_values().collect()
}

fn top_models(stats: &StatsData) -> Vec<ModelUsageSummary> {
    let mut models: Vec<ModelUsageSummary> = consolidated_model_usage(&stats.models)
        .iter()
        .map(|entry| ModelUsageSummary {
            provider_id: entry.model.provider_id.clone(),
            model_id: entry.model.id.clone(),
            name: model_display_name(&entry.model.provider_id, &entry.model.id),
            cost: entry.cost,
            steps: entry.steps,
            tokens: entry.tokens.clone(),
        })
        .collect();
    models.sort_by(|left, right| {
        right
            .cost
            .partial_cmp(&left.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.provider_id.cmp(&right.provider_id))
            .then_with(|| left.model_id.cmp(&right.model_id))
    });
    models
}

fn tool_summaries(stats: &StatsData) -> Vec<ToolUsageSummary> {
    let Some(usage) = stats.tools.as_ref().and_then(|tools| tools.usage.as_ref()) else {
        return Vec::new();
    };

    let mut tools: Vec<ToolUsageSummary> = usage
        .iter()
        .map(|tool| ToolUsageSummary {
            name: tool.name.clone(),
            calls: tool.calls,
            succeeded: tool.succeeded,
            failed: tool.failed,
            unfinished: tool.unfinished,
            duration_p50: tool.duration_p50,
        })
        .collect();
    tools.sort_by(|left, right| right.calls.cmp(&left.calls));
    tools.truncate(MAX_TOOLS);
    tools
}

/// Agrega una ventana al estilo de la consola: el porcentaje corresponde al
/// modelo OpenCode Go con mayor proporción de su propio límite (la restricción
/// que antes bloquearía el uso). `spent` es el gasto de ese modelo; si ningún
/// modelo con límite publicado tiene uso, se informa el gasto Go total.
fn window_aggregate(
    stats: &StatsData,
    window: (i64, i64),
    plan: UsagePlan,
    fraction: f64,
    resets_at: Option<i64>,
    reset_is_estimate: bool,
) -> WindowAggregate {
    let mut binding: Option<(String, f64, f64, String, f64)> = None;
    let mut go_spent = 0.0;

    for entry in consolidated_model_usage(&stats.models) {
        if !entry.model.provider_id.eq_ignore_ascii_case(GO_PROVIDER_ID) {
            continue;
        }
        go_spent += entry.cost;

        let Some(reference) = GO_MODEL_LIMITS
            .iter()
            .find(|(id, _, _)| id.eq_ignore_ascii_case(&entry.model.id))
            .map(|(_, _, reference)| *reference)
        else {
            continue;
        };
        let Some(monthly_limit) = plan.reference_limit(reference) else {
            continue;
        };
        if monthly_limit <= 0.0 {
            continue;
        }

        let limit = monthly_limit * fraction;
        let ratio = entry.cost / limit;
        let better = match binding.as_ref() {
            None => true,
            Some((_, _, best_ratio, _, best_spent)) => {
                ratio > *best_ratio || (ratio == *best_ratio && entry.cost > *best_spent)
            }
        };
        if better {
            binding = Some((
                entry.model.id.clone(),
                limit,
                ratio,
                model_display_name(&entry.model.provider_id, &entry.model.id),
                entry.cost,
            ));
        }
    }

    match binding {
        Some((model_id, limit, ratio, name, spent)) => WindowAggregate {
            from: window.0,
            to: window.1,
            spent,
            allowance: Some(limit),
            ratio: Some(ratio),
            model_name: Some(name),
            model_id: Some(model_id),
            limit_status: LimitStatus::Limited,
            resets_at,
            reset_is_estimate,
        },
        None => WindowAggregate {
            from: window.0,
            to: window.1,
            spent: go_spent,
            allowance: None,
            ratio: None,
            model_name: None,
            model_id: None,
            limit_status: LimitStatus::Unknown,
            resets_at,
            reset_is_estimate,
        },
    }
}

/// Estima cuándo la ventana móvil deja de contar actividad: el bucket
/// positivo más antiguo sale de la ventana en `inicio + ventana`, con una
/// resolución de medio bucket. Si el bucket empezó antes de la ventana, solo
/// queda su parte dentro de ella y la salida es a medio bucket de ahora.
/// La consola de OpenCode muestra el valor exacto.
fn estimate_reset(
    buckets: &[UsageBucket],
    window_ms: i64,
    bucket_ms: i64,
    now: i64,
) -> Option<i64> {
    let window_start = now - window_ms;
    let oldest = buckets
        .iter()
        .filter(|bucket| bucket.cost > 0.0 && bucket.end > window_start)
        .min_by_key(|bucket| bucket.start)?;
    let estimate = oldest.start.max(window_start) + window_ms + bucket_ms / 2;
    Some(estimate.max(now + 60_000))
}

fn progress_slice(spent: f64, monthly_limit: Option<f64>, fraction: f64) -> ProgressSlice {
    match monthly_limit {
        Some(monthly) if monthly > 0.0 => {
            let limit = monthly * fraction;
            ProgressSlice {
                spent,
                limit: Some(limit),
                ratio: Some(spent / limit),
            }
        }
        _ => ProgressSlice {
            spent,
            limit: None,
            ratio: None,
        },
    }
}

fn build_go_limits(
    five_hour: &StatsData,
    week: &StatsData,
    month: &StatsData,
    plan: UsagePlan,
) -> Vec<GoModelProgress> {
    #[derive(Default)]
    struct Accumulated {
        five_hour: f64,
        week: f64,
        month: f64,
        steps: u64,
        tokens: TokenTotals,
    }

    let mut accumulated: HashMap<(String, String), Accumulated> = HashMap::new();
    for (slot_index, stats) in [(0u8, five_hour), (1, week), (2, month)] {
        for entry in consolidated_model_usage(&stats.models) {
            if !entry.model.provider_id.eq_ignore_ascii_case(GO_PROVIDER_ID) {
                continue;
            }
            let key = (
                entry.model.provider_id.trim().to_ascii_lowercase(),
                entry.model.id.trim().to_ascii_lowercase(),
            );
            let slot = accumulated.entry(key).or_default();
            match slot_index {
                0 => slot.five_hour = entry.cost,
                1 => slot.week = entry.cost,
                _ => {
                    slot.month = entry.cost;
                    slot.steps = entry.steps;
                    slot.tokens = entry.tokens.clone();
                }
            }
        }
    }

    let mut items: Vec<GoModelProgress> = accumulated
        .into_iter()
        .map(|((provider_id, model_id), usage)| {
            let reference = GO_MODEL_LIMITS
                .iter()
                .find(|(id, _, _)| id.eq_ignore_ascii_case(&model_id))
                .map(|(_, _, reference)| *reference);
            let (limit_status, monthly_limit, name) = match reference {
                Some(reference) => (
                    if matches!(reference, LimitReference::Unlimited) {
                        LimitStatus::Unlimited
                    } else {
                        LimitStatus::Limited
                    },
                    match reference {
                        LimitReference::Unlimited => None,
                        LimitReference::Limited { .. } => plan.reference_limit(reference),
                    },
                    GO_MODEL_LIMITS
                        .iter()
                        .find(|(id, _, _)| id.eq_ignore_ascii_case(&model_id))
                        .map(|(_, name, _)| (*name).to_string())
                        .unwrap_or_else(|| model_id.clone()),
                ),
                None => (LimitStatus::Unknown, None, model_id.clone()),
            };

            GoModelProgress {
                provider_id,
                model_id,
                name,
                limit_status,
                monthly_limit,
                five_hour: progress_slice(usage.five_hour, monthly_limit, FIVE_HOUR_FRACTION),
                week: progress_slice(usage.week, monthly_limit, WEEK_FRACTION),
                month: progress_slice(usage.month, monthly_limit, 1.0),
                steps: usage.steps,
                tokens: usage.tokens,
            }
        })
        .collect();

    items.sort_by(|left, right| {
        let left_ratio = left.month.ratio.unwrap_or(-1.0);
        let right_ratio = right.month.ratio.unwrap_or(-1.0);
        right_ratio
            .partial_cmp(&left_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .month
                    .spent
                    .partial_cmp(&left.month.spent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    items
}

/// Recorre los inicios de hora locales hacia atrás y los devuelve en orden
/// cronológico. `utc_offset_minutes` es el valor de `Date.getTimezoneOffset()`
/// (minutos que se suman a la hora local para obtener UTC).
fn hour_buckets(now_ms: i64, utc_offset_minutes: i64, count: i64) -> Vec<(i64, i64)> {
    let offset = utc_offset_minutes * 60_000;
    let current_start = (now_ms - offset).div_euclid(HOUR_MS) * HOUR_MS + offset;
    (0..count)
        .map(|index| current_start - (count - 1 - index) * HOUR_MS)
        .map(|start| (start, start + HOUR_MS))
        .collect()
}

fn day_buckets(now_ms: i64, utc_offset_minutes: i64, count: i64) -> Vec<(i64, i64)> {
    let offset = utc_offset_minutes * 60_000;
    let current_start = (now_ms - offset).div_euclid(DAY_MS) * DAY_MS + offset;
    (0..count)
        .map(|index| current_start - (count - 1 - index) * DAY_MS)
        .map(|start| (start, start + DAY_MS))
        .collect()
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = if month <= 2 { year - 1 } else { year };
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Inicio del siguiente mes calendario UTC: cuándo reinicia el límite mensual.
fn utc_next_month_start(now_ms: i64) -> i64 {
    let (year, month, _) = civil_from_days(now_ms.div_euclid(DAY_MS));
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    days_from_civil(next_year, next_month, 1) * DAY_MS
}

pub async fn fetch_overview(
    base_url: &str,
    username: &str,
    password: &str,
    plan: &str,
    utc_offset_minutes: i64,
    timezone: &str,
    local_day_start_ms: i64,
    local_month_start_ms: i64,
    days: i64,
) -> Result<UsageOverview, String> {
    let plan = UsagePlan::parse(plan)?;
    let base_url = opencode::validate_local_base_url(base_url)?;
    let client = usage_client()?;
    let now = now_millis()?;
    let utc_offset_minutes = utc_offset_minutes.clamp(-16 * 60, 16 * 60);
    let timezone = timezone.trim();
    if timezone.is_empty()
        || timezone.len() > 100
        || timezone.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '/' | '_' | '-' | '+'))
        })
    {
        return Err("La zona horaria local indicada no es válida".to_string());
    }
    if local_day_start_ms <= 0 || local_day_start_ms > now || now - local_day_start_ms > 2 * DAY_MS
    {
        return Err("El inicio del día local indicado no es válido".to_string());
    }
    if local_month_start_ms <= 0
        || local_month_start_ms > now
        || now - local_month_start_ms > 32 * DAY_MS
    {
        return Err("El inicio del mes local indicado no es válido".to_string());
    }
    let days = if days <= 0 {
        DEFAULT_DAILY_BUCKETS
    } else {
        days.clamp(MIN_DAILY_BUCKETS, MAX_DAILY_BUCKETS)
    };

    let today_range = (local_day_start_ms, now);
    let five_hour_range = (now - FIVE_HOURS_MS, now);
    let week_range = (now - WEEK_MS, now);
    let month_range = (local_month_start_ms, now);

    let (today, five_hour, week, month) = tokio::join!(
        fetch_stats(
            &client,
            &base_url,
            username,
            password,
            Some(today_range),
            "none",
            timezone,
        ),
        fetch_stats(
            &client,
            &base_url,
            username,
            password,
            Some(five_hour_range),
            "none",
            timezone,
        ),
        fetch_stats(
            &client,
            &base_url,
            username,
            password,
            Some(week_range),
            "none",
            timezone,
        ),
        fetch_stats(
            &client,
            &base_url,
            username,
            password,
            Some(month_range),
            "none",
            timezone,
        ),
    );

    let today = today?;
    let five_hour = five_hour?;
    let week = week?;
    let month = month?;

    let mut warnings = Vec::new();

    let hour_ranges = hour_buckets(now, utc_offset_minutes, HOURLY_BUCKETS);
    let day_ranges = day_buckets(now, utc_offset_minutes, days);
    let hourly = buckets_from_stats(
        &hour_ranges,
        fetch_stats_batch(
            &client,
            &base_url,
            username,
            password,
            &hour_ranges,
            timezone,
        )
        .await,
        &mut warnings,
        "horas",
    );
    let daily = buckets_from_stats(
        &day_ranges,
        fetch_stats_batch(
            &client,
            &base_url,
            username,
            password,
            &day_ranges,
            timezone,
        )
        .await,
        &mut warnings,
        "días",
    );

    let lifetime = match fetch_stats(
        &client, &base_url, username, password, None, "none", timezone,
    )
    .await
    {
        Ok(stats) => totals_from_stats(&stats),
        Err(error) => {
            warnings.push(format!("No se pudo cargar el histórico: {error}"));
            UsageTotals {
                cost: 0.0,
                sessions: 0,
                subagents: 0,
                prompts: 0,
                steps: 0,
                tokens: TokenTotals::default(),
                active_days: 0,
                streak: 0,
            }
        }
    };

    let sessions = match fetch_recent_sessions(&client, &base_url, username, password).await {
        Ok(sessions) => sessions,
        Err(error) => {
            warnings.push(format!(
                "No se pudieron cargar las sesiones recientes: {error}"
            ));
            Vec::new()
        }
    };

    let aggregates = UsageAggregates {
        rolling: window_aggregate(
            &five_hour,
            five_hour_range,
            plan,
            FIVE_HOUR_FRACTION,
            estimate_reset(&hourly, FIVE_HOURS_MS, HOUR_MS, now),
            true,
        ),
        weekly: window_aggregate(
            &week,
            week_range,
            plan,
            WEEK_FRACTION,
            estimate_reset(&daily, WEEK_MS, DAY_MS, now),
            true,
        ),
        monthly: window_aggregate(
            &month,
            month_range,
            plan,
            1.0,
            Some(utc_next_month_start(now)),
            false,
        ),
    };

    Ok(UsageOverview {
        generated_at: now,
        plan: plan.as_str().to_string(),
        today: UsageWindow {
            from: today_range.0,
            to: today_range.1,
            totals: totals_from_stats(&today),
        },
        five_hour: UsageWindow {
            from: five_hour_range.0,
            to: five_hour_range.1,
            totals: totals_from_stats(&five_hour),
        },
        week: UsageWindow {
            from: week_range.0,
            to: week_range.1,
            totals: totals_from_stats(&week),
        },
        month: UsageWindow {
            from: month_range.0,
            to: month_range.1,
            totals: totals_from_stats(&month),
        },
        model_usage: ModelUsagePeriods {
            today: top_models(&today),
            five_hour: top_models(&five_hour),
            week: top_models(&week),
            month: top_models(&month),
        },
        aggregates,
        hourly,
        daily,
        lifetime,
        go_limits: build_go_limits(&five_hour, &week, &month, plan),
        top_models: top_models(&month),
        tools: tool_summaries(&month),
        sessions,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stats(cost: f64, steps: u64, models: &[(&str, &str, f64, u64)]) -> StatsData {
        StatsData {
            sessions: 1,
            subagents: 0,
            prompts: 1,
            steps,
            tokens: TokenTotals::default(),
            cost,
            tools: None,
            active_days: 1,
            streak: 1,
            models: models
                .iter()
                .map(|(provider_id, model_id, cost, steps)| ApiModelUsage {
                    model: ApiModelRef {
                        id: (*model_id).to_string(),
                        provider_id: (*provider_id).to_string(),
                    },
                    steps: *steps,
                    tokens: TokenTotals::default(),
                    cost: *cost,
                })
                .collect(),
        }
    }

    #[test]
    fn builds_local_hour_and_day_buckets_in_chronological_order() {
        // 2026-09-28T12:34:56Z con UTC-3: la hora local actual inicia a las 12:00Z
        // y el día local actual inicia a las 03:00Z.
        let now = 1_790_598_896_000;
        let hours = hour_buckets(now, 180, 24);
        assert_eq!(hours.len(), 24);
        assert_eq!(
            hours.last().map(|(start, _)| *start),
            Some(1_790_596_800_000)
        );
        assert!(hours
            .windows(2)
            .all(|pair| pair[1].0 - pair[0].0 == HOUR_MS));

        let days = day_buckets(now, 180, 14);
        assert_eq!(days.len(), 14);
        assert_eq!(
            days.last().map(|(start, _)| *start),
            Some(1_790_564_400_000)
        );
        assert!(days.windows(2).all(|pair| pair[1].0 - pair[0].0 == DAY_MS));
    }

    #[test]
    fn resolves_limits_from_the_go_reference_table() {
        let reference = |model_id: &str| {
            GO_MODEL_LIMITS
                .iter()
                .find(|(id, _, _)| *id == model_id)
                .map(|(_, _, reference)| *reference)
        };

        assert_eq!(
            UsagePlan::Go.reference_limit(reference("deepseek-v4.1-flash").expect("listed")),
            Some(60.0)
        );
        assert_eq!(
            UsagePlan::GoPlus.reference_limit(reference("deepseek-v4.1-flash").expect("listed")),
            Some(120.0)
        );
        assert!(matches!(
            reference("space-bunny-free"),
            Some(LimitReference::Unlimited)
        ));
        assert!(reference("modelo-desconocido").is_none());
    }

    #[test]
    fn joins_windows_into_per_model_limit_progress() {
        let five_hour = sample_stats(
            3.0,
            30,
            &[
                ("opencode-go", "deepseek-v4.1-flash", 3.0, 30),
                ("minimax", "MiniMax-M3", 1.0, 5),
            ],
        );
        let week = sample_stats(9.0, 90, &[("opencode-go", "deepseek-v4.1-flash", 9.0, 90)]);
        let month = sample_stats(
            15.0,
            150,
            &[
                ("opencode-go", "deepseek-v4.1-flash", 15.0, 150),
                ("opencode-go", "union-alpha", 1.0, 10),
            ],
        );

        let limits = build_go_limits(&five_hour, &week, &month, UsagePlan::Go);
        assert_eq!(limits.len(), 2);

        let deepseek = &limits[0];
        assert_eq!(deepseek.model_id, "deepseek-v4.1-flash");
        assert_eq!(deepseek.name, "DeepSeek V4.1 Flash");
        assert_eq!(deepseek.limit_status, LimitStatus::Limited);
        assert_eq!(deepseek.monthly_limit, Some(60.0));
        assert_eq!(deepseek.five_hour.limit, Some(12.0));
        assert!((deepseek.five_hour.ratio.unwrap_or_default() - 0.25).abs() < 1e-9);
        assert_eq!(deepseek.week.limit, Some(30.0));
        assert!((deepseek.month.ratio.unwrap_or_default() - 0.25).abs() < 1e-9);

        let unknown = &limits[1];
        assert_eq!(unknown.model_id, "union-alpha");
        assert_eq!(unknown.limit_status, LimitStatus::Unknown);
        assert_eq!(unknown.month.ratio, None);
    }

    #[test]
    fn consolidates_duplicate_model_entries_case_insensitively() {
        let stats = sample_stats(
            3.0,
            7,
            &[
                ("opencode-go", "gpt-6-luna", 1.0, 3),
                ("OpenCode-Go", "GPT-6-LUNA", 2.0, 4),
            ],
        );

        let models = top_models(&stats);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "GPT 6 Luna");
        assert_eq!(models[0].cost, 3.0);
        assert_eq!(models[0].steps, 7);

        let limits = build_go_limits(&stats, &stats, &stats, UsagePlan::Go);
        assert_eq!(limits.len(), 1);
        assert_eq!(limits[0].month.spent, 3.0);

        let aggregate = window_aggregate(&stats, (0, 1), UsagePlan::Go, 1.0, None, false);
        assert_eq!(aggregate.model_id.as_deref(), Some("gpt-6-luna"));
        assert_eq!(aggregate.spent, 3.0);
    }

    #[test]
    fn reports_unlimited_models_without_a_dollar_limit() {
        let stats = sample_stats(2.0, 10, &[("opencode-go", "space-bunny-free", 2.0, 10)]);
        let limits = build_go_limits(&stats, &stats, &stats, UsagePlan::Go);
        assert_eq!(limits.len(), 1);
        assert_eq!(limits[0].limit_status, LimitStatus::Unlimited);
        assert_eq!(limits[0].monthly_limit, None);
        assert_eq!(limits[0].month.ratio, None);
        assert_eq!(limits[0].month.spent, 2.0);
    }

    #[test]
    fn parses_supported_plan_names() {
        assert!(UsagePlan::parse("go").is_ok());
        assert!(UsagePlan::parse("go_plus").is_ok());
        assert!(UsagePlan::parse(" Go Plus ").is_ok());
        assert!(UsagePlan::parse("enterprise").is_err());
    }

    #[test]
    fn serializes_usage_payloads_with_camel_case_keys() {
        let stats = sample_stats(3.0, 30, &[("opencode-go", "deepseek-v4.1-flash", 3.0, 30)]);
        let limits = build_go_limits(&stats, &stats, &stats, UsagePlan::Go);
        let json = serde_json::to_value(&limits[0]).expect("limit should serialize");

        assert!(json.get("monthlyLimit").is_some());
        assert!(json.get("fiveHour").is_some());
        assert!(json.get("limitStatus").is_some());
        assert_eq!(json["fiveHour"]["limit"], serde_json::json!(12.0));
        assert_eq!(json["limitStatus"], serde_json::json!("limited"));

        let totals =
            serde_json::to_value(totals_from_stats(&stats)).expect("totals should serialize");
        assert_eq!(totals["tokens"]["cache"]["read"], serde_json::json!(0.0));
    }

    #[test]
    fn computes_the_next_utc_month_start() {
        assert_eq!(utc_next_month_start(1_790_596_800_000), 1_790_812_800_000);
    }

    #[test]
    fn aggregates_use_the_tightest_model_per_window() {
        let stats = sample_stats(
            4.5,
            20,
            &[
                ("opencode-go", "deepseek-v4.1-flash", 3.0, 10),
                ("opencode-go", "gpt-6-luna", 1.5, 10),
                ("minimax", "MiniMax-M3", 2.0, 5),
            ],
        );
        let aggregate = window_aggregate(
            &stats,
            (0, 1),
            UsagePlan::Go,
            FIVE_HOUR_FRACTION,
            None,
            true,
        );

        // GPT 6 Luna: 1.5 / (15 * 0.2) = 0.5 supera a DeepSeek: 3 / (60 * 0.2) = 0.25.
        assert_eq!(aggregate.limit_status, LimitStatus::Limited);
        assert_eq!(aggregate.model_id.as_deref(), Some("gpt-6-luna"));
        assert_eq!(aggregate.allowance, Some(3.0));
        assert!((aggregate.ratio.unwrap_or_default() - 0.5).abs() < 1e-9);
        assert_eq!(aggregate.spent, 1.5);
    }

    #[test]
    fn aggregate_without_published_limits_reports_total_go_spend() {
        let stats = sample_stats(
            2.5,
            12,
            &[
                ("opencode-go", "union-alpha", 1.0, 6),
                ("opencode-go", "space-bunny-free", 1.5, 6),
            ],
        );
        let aggregate = window_aggregate(&stats, (0, 1), UsagePlan::Go, 1.0, None, false);

        assert_eq!(aggregate.limit_status, LimitStatus::Unknown);
        assert_eq!(aggregate.ratio, None);
        assert_eq!(aggregate.allowance, None);
        assert_eq!(aggregate.spent, 2.5);
    }

    #[test]
    fn estimates_window_resets_from_bucket_activity() {
        let now = 1_790_596_800_000;
        let buckets = vec![
            UsageBucket {
                start: now - HOUR_MS,
                end: now,
                cost: 0.4,
                steps: 3,
            },
            UsageBucket {
                start: now - 3 * HOUR_MS,
                end: now - 2 * HOUR_MS,
                cost: 0.2,
                steps: 1,
            },
        ];

        // El bucket positivo más antiguo (hace 3 h) sale de la ventana en 2 h,
        // más medio bucket de resolución: 2 h 30 min.
        assert_eq!(
            estimate_reset(&buckets, FIVE_HOURS_MS, HOUR_MS, now),
            Some(now + 150 * 60 * 1000)
        );
        // Sin actividad, no hay reinicio estimable.
        assert_eq!(estimate_reset(&[], FIVE_HOURS_MS, HOUR_MS, now), None);

        // Un bucket que empezó antes de la ventana solo aporta su parte dentro
        // de ella: la salida se estima a medio bucket de ahora.
        let partial = vec![UsageBucket {
            start: now - FIVE_HOURS_MS - HOUR_MS / 2,
            end: now - 4 * HOUR_MS - HOUR_MS / 2,
            cost: 0.3,
            steps: 1,
        }];
        assert_eq!(
            estimate_reset(&partial, FIVE_HOURS_MS, HOUR_MS, now),
            Some(now + HOUR_MS / 2)
        );
    }

    #[test]
    #[ignore = "smoke test against the locally installed OpenCode CLI"]
    fn loads_a_real_usage_overview_from_the_installed_cli() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("an unused local port should be available");
        let port = listener
            .local_addr()
            .expect("the listener address should be available")
            .port();
        drop(listener);

        tauri::async_runtime::block_on(async {
            let manager = crate::opencode_process::OpenCodeProcessManager::default();
            let started = manager
                .start(&format!("http://127.0.0.1:{port}"))
                .await
                .expect("the installed OpenCode server should start and authenticate");
            let now = super::now_millis().expect("the system clock should be valid");
            let today_start = now.div_euclid(super::DAY_MS) * super::DAY_MS;
            let month_start = now - 28 * super::DAY_MS;

            let overview = super::fetch_overview(
                &started.base_url,
                &started.username,
                &started.password,
                "go",
                180,
                "UTC",
                today_start,
                month_start,
                14,
            )
            .await
            .expect("the usage overview should load from a real server");

            assert_eq!(overview.plan, "go");
            assert_eq!(overview.hourly.len(), 24);
            assert_eq!(overview.daily.len(), 14);
            assert!(overview.five_hour.totals.cost >= 0.0);
            assert!(overview.generated_at > 0);
            assert!(
                overview.warnings.is_empty(),
                "unexpected warnings: {:?}",
                overview.warnings
            );

            let json = serde_json::to_value(&overview).expect("overview should serialize");
            for key in [
                "generatedAt",
                "today",
                "fiveHour",
                "week",
                "month",
                "modelUsage",
                "aggregates",
                "hourly",
                "daily",
                "lifetime",
                "goLimits",
                "topModels",
                "tools",
                "sessions",
            ] {
                assert!(json.get(key).is_some(), "missing key {key}");
            }

            manager
                .stop()
                .await
                .expect("the managed process should stop");
        });
    }
}
