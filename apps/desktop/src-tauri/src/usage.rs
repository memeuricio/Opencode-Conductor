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
const MAX_SESSIONS: usize = 12;
const GO_PROVIDER_ID: &str = "opencode-go";

/// Nombres publicados para los modelos de OpenCode Go. El resto de modelos se
/// muestran con su identificador; la app no infiere cuotas ni límites que la API
/// local no exponga.
const GO_MODEL_NAMES: &[(&str, &str)] = &[
    ("glm-5.3-flash", "GLM-5.3-Flash"),
    ("glm-5.3", "GLM-5.3"),
    ("glm-5.2", "GLM-5.2"),
    ("kimi-k3", "Kimi K3"),
    ("kimi-k2.7-code", "Kimi K2.7 Code"),
    ("kimi-k2.6", "Kimi K2.6"),
    ("longcat-2.0", "LongCat-2.0"),
    ("longcat-2.5-preview-free", "LongCat 2.5 Preview Free"),
    ("mimo-v2.6-flash", "MiMo-V2.6-Flash"),
    ("mimo-v2.6-pro", "MiMo-V2.6-Pro"),
    ("mimo-v2.5", "MiMo-V2.5"),
    ("mimo-v2.5-pro", "MiMo-V2.5-Pro"),
    ("minimax-m3", "MiniMax-M3"),
    ("minimax-m2.7", "MiniMax-M2.7"),
    ("muse-spark-1.3-contributor", "Muse Spark 1.3 Contributor"),
    ("muse-spark-1.2-contributor", "Muse Spark 1.2 Contributor"),
    ("qwen3.8-max", "Qwen3.8 Max"),
    ("qwen3.8-flash", "Qwen3.8 Flash"),
    ("qwen3.7-plus", "Qwen3.7 Plus"),
    ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash"),
    ("deepseek-v4-pro", "DeepSeek V4 Pro"),
    ("deepseek-v4-flash", "DeepSeek V4 Flash"),
    (
        "deepseek-v4-flash-vision-exp",
        "DeepSeek V4 Flash Vision Exp",
    ),
    ("hy4-preview", "Hy4 preview"),
    ("hy3", "Hy3"),
    ("space-bunny-free", "Space Bunny Free"),
    ("grok-4.7", "Grok 4.7"),
    ("grok-4.6", "Grok 4.6"),
    ("gpt-6-luna", "GPT 6 Luna"),
    ("gpt-5.6-luna", "GPT 5.6 Luna"),
];

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
pub struct UsageOverview {
    pub generated_at: i64,
    pub today: UsageWindow,
    pub five_hour: UsageWindow,
    pub week: UsageWindow,
    pub month: UsageWindow,
    pub model_usage: ModelUsagePeriods,
    pub hourly: Vec<UsageBucket>,
    pub daily: Vec<UsageBucket>,
    pub lifetime: UsageTotals,
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
        // El resumen de herramientas no se muestra en la app; pedirlo desactivado
        // reduce el tamaño de la respuesta del endpoint experimental.
        query.append_pair("tools", "none");
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

    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated.unwrap_or(0)));
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
                    &timezone,
                )
                .await;
                (index, result)
            });
        }

        while let Some(joined) = joined_set.join_next().await {
            if let Ok((index, result)) = joined {
                slots[index] = Some(result);
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
        if let Some((_, name)) = GO_MODEL_NAMES
            .iter()
            .find(|(id, _)| id.eq_ignore_ascii_case(model_id))
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

#[allow(clippy::too_many_arguments)]
pub async fn fetch_overview(
    base_url: &str,
    username: &str,
    password: &str,
    utc_offset_minutes: i64,
    timezone: &str,
    local_day_start_ms: i64,
    local_month_start_ms: i64,
    days: i64,
) -> Result<UsageOverview, String> {
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
            timezone,
        ),
        fetch_stats(
            &client,
            &base_url,
            username,
            password,
            Some(five_hour_range),
            timezone,
        ),
        fetch_stats(
            &client,
            &base_url,
            username,
            password,
            Some(week_range),
            timezone,
        ),
        fetch_stats(
            &client,
            &base_url,
            username,
            password,
            Some(month_range),
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

    let lifetime = match fetch_stats(&client, &base_url, username, password, None, timezone).await {
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

    Ok(UsageOverview {
        generated_at: now,
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
        hourly,
        daily,
        lifetime,
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
    }

    #[test]
    fn serializes_usage_payloads_with_camel_case_keys() {
        let stats = sample_stats(3.0, 30, &[("opencode-go", "deepseek-v4.1-flash", 3.0, 30)]);
        let totals =
            serde_json::to_value(totals_from_stats(&stats)).expect("totals should serialize");
        assert_eq!(totals["tokens"]["cache"]["read"], serde_json::json!(0.0));
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
                180,
                "UTC",
                today_start,
                month_start,
                14,
            )
            .await
            .expect("the usage overview should load from a real server");

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
                "hourly",
                "daily",
                "lifetime",
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
