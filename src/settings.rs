//! Settings page + JSON API for agent command configuration.

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Form, Json, Router};
use serde::Deserialize;

use crate::db::AgentSettings;
use crate::error::AppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings", get(settings_page).post(settings_save))
        .route("/api/settings", get(api_get).post(api_post))
}

#[derive(Template, WebTemplate)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    cursor_cmd: String,
    opencode_cmd: String,
    cursor_args: String,
    opencode_args: String,
    cursor_enabled: bool,
    opencode_enabled: bool,
    install_notes: String,
    env_cursor: String,
    env_opencode: String,
    db_path: String,
    saved: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SettingsForm {
    pub cursor_cmd: String,
    pub opencode_cmd: String,
    pub cursor_args: String,
    pub opencode_args: String,
    pub cursor_enabled: Option<String>,
    pub opencode_enabled: Option<String>,
    pub install_notes: String,
}

#[derive(Debug, Deserialize)]
pub struct SettingsJson {
    pub cursor_cmd: Option<String>,
    pub opencode_cmd: Option<String>,
    pub cursor_args: Option<String>,
    pub opencode_args: Option<String>,
    pub cursor_enabled: Option<bool>,
    pub opencode_enabled: Option<bool>,
    pub install_notes: Option<String>,
    pub last_opened_project: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SettingsQuery {
    #[serde(default)]
    saved: Option<String>,
}

async fn settings_page(
    State(state): State<AppState>,
    Query(q): Query<SettingsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let saved = q.saved.as_deref() == Some("1");
    Ok(render_page(&state, saved, None)?)
}

async fn settings_save(
    State(state): State<AppState>,
    Form(form): Form<SettingsForm>,
) -> Result<Response, AppError> {
    let s = AgentSettings {
        cursor_cmd: nonempty(form.cursor_cmd),
        opencode_cmd: nonempty(form.opencode_cmd),
        cursor_args: form.cursor_args,
        opencode_args: form.opencode_args,
        cursor_enabled: form.cursor_enabled.is_some(),
        opencode_enabled: form.opencode_enabled.is_some(),
        last_opened_project: state
            .db
            .load_agent_settings()
            .map_err(|e| AppError::Other(e))?
            .last_opened_project,
        install_notes: form.install_notes,
    };
    if let Err(e) = state.db.save_agent_settings(&s) {
        return Ok(render_page(&state, false, Some(e.to_string()))?.into_response());
    }
    Ok(Redirect::to("/settings?saved=1").into_response())
}

async fn api_get(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let s = state
        .db
        .load_agent_settings()
        .map_err(|e| AppError::Other(e))?;
    Ok(Json(public_settings(&state, &s)))
}

async fn api_post(
    State(state): State<AppState>,
    Json(body): Json<SettingsJson>,
) -> Result<impl IntoResponse, AppError> {
    let mut s = state
        .db
        .load_agent_settings()
        .map_err(|e| AppError::Other(e))?;
    if let Some(v) = body.cursor_cmd {
        s.cursor_cmd = nonempty(v);
    }
    if let Some(v) = body.opencode_cmd {
        s.opencode_cmd = nonempty(v);
    }
    if let Some(v) = body.cursor_args {
        s.cursor_args = v;
    }
    if let Some(v) = body.opencode_args {
        s.opencode_args = v;
    }
    if let Some(v) = body.cursor_enabled {
        s.cursor_enabled = v;
    }
    if let Some(v) = body.opencode_enabled {
        s.opencode_enabled = v;
    }
    if let Some(v) = body.install_notes {
        s.install_notes = v;
    }
    if let Some(v) = body.last_opened_project {
        s.last_opened_project = nonempty(v);
    }
    state
        .db
        .save_agent_settings(&s)
        .map_err(|e| AppError::Other(e))?;
    Ok(Json(public_settings(&state, &s)))
}

fn public_settings(state: &AppState, s: &AgentSettings) -> serde_json::Value {
    serde_json::json!({
        "cursor_cmd": s.cursor_cmd,
        "opencode_cmd": s.opencode_cmd,
        "cursor_args": s.cursor_args,
        "opencode_args": s.opencode_args,
        "cursor_enabled": s.cursor_enabled,
        "opencode_enabled": s.opencode_enabled,
        "last_opened_project": s.last_opened_project,
        "install_notes": s.install_notes,
        "env_defaults": {
            "cursor_cmd": state.config.cursor_cmd,
            "opencode_cmd": state.config.opencode_cmd,
        },
        "resolved": {
            "cursor": s.resolve_cmd("cursor", &state.config.cursor_cmd),
            "opencode": s.resolve_cmd("opencode", &state.config.opencode_cmd),
        },
        "database": state.db.path().display().to_string(),
    })
}

fn render_page(
    state: &AppState,
    saved: bool,
    error: Option<String>,
) -> Result<SettingsTemplate, AppError> {
    let s = state
        .db
        .load_agent_settings()
        .map_err(|e| AppError::Other(e))?;
    Ok(SettingsTemplate {
        cursor_cmd: s.cursor_cmd.unwrap_or_default(),
        opencode_cmd: s.opencode_cmd.unwrap_or_default(),
        cursor_args: s.cursor_args,
        opencode_args: s.opencode_args,
        cursor_enabled: s.cursor_enabled,
        opencode_enabled: s.opencode_enabled,
        install_notes: s.install_notes,
        env_cursor: state.config.cursor_cmd.clone(),
        env_opencode: state.config.opencode_cmd.clone(),
        db_path: state.db.path().display().to_string(),
        saved,
        error,
    })
}

fn nonempty(s: String) -> Option<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Effective cursor/opencode launch strings for sync/tmux.
pub fn effective_cmds(state: &AppState) -> (String, String) {
    let s = state.db.load_agent_settings().unwrap_or_default();
    (
        s.resolve_cmd("cursor", &state.config.cursor_cmd),
        s.resolve_cmd("opencode", &state.config.opencode_cmd),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty_filters() {
        assert!(nonempty("  ".into()).is_none());
        assert_eq!(nonempty(" x ".into()).as_deref(), Some("x"));
    }
}
