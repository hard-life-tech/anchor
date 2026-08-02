//! In-browser terminal: xterm.js frontend + PTY WebSocket attached to tmux.

use std::io::{Read, Write};
use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::error::AppError;
use crate::project_store;
use crate::tmux;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects/{slug}/terminal", get(terminal_page))
        .route("/ws/terminal/{slug}/{agent}", get(ws_upgrade))
}

#[derive(Template, WebTemplate)]
#[template(path = "terminal.html")]
struct TerminalTemplate {
    repo: String,
    agent: String,
    session: String,
    window_exists: bool,
    on_disk: bool,
    pane_label: String,
    repo_js: String,
    agent_js: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalQuery {
    #[serde(default = "default_agent")]
    pub agent: String,
}

fn default_agent() -> String {
    "cursor".into()
}

async fn terminal_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<TerminalQuery>,
) -> Result<impl IntoResponse, AppError> {
    let agent = normalize_agent(&q.agent)?;
    let on_disk = project_store::ensure_db_has_project(
        &state.db,
        &state.config.projects_dir,
        &slug,
    )
    .await
    .map_err(AppError::Other)?
    .is_some()
        || state.config.projects_dir.join(&slug).exists();

    let window_exists = if on_disk {
        tmux::window_exists(&state.config.tmux_session, &slug)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    if on_disk {
        let _ = state.db.set("last_opened_project", &slug);
    }

    Ok(TerminalTemplate {
        repo_js: serde_json::to_string(&slug).unwrap_or_else(|_| "\"\"".into()),
        agent_js: serde_json::to_string(&agent).unwrap_or_else(|_| "\"\"".into()),
        repo: slug,
        agent: agent.to_string(),
        session: state.config.tmux_session.clone(),
        window_exists,
        on_disk,
        pane_label: agent.to_string(),
    })
}

async fn ws_upgrade(
    State(state): State<AppState>,
    Path((slug, agent)): Path<(String, String)>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let agent = normalize_agent(&agent)?.to_string();
    let known = project_store::ensure_db_has_project(
        &state.db,
        &state.config.projects_dir,
        &slug,
    )
    .await
    .map_err(AppError::Other)?
    .is_some()
        || state.config.projects_dir.join(&slug).exists();
    if !known {
        return Err(AppError::NotFound(format!("project not found: {slug}")));
    }
    if !tmux::window_exists(&state.config.tmux_session, &slug)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::NotFound(format!(
            "tmux window missing for {slug} — sync the project first"
        )));
    }

    let pane = pane_index(&agent);
    let session = state.config.tmux_session.clone();
    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_socket(socket, session, slug, pane).await {
            tracing::warn!(
                "terminal ws closed: {}",
                crate::error::redact_secrets(&e.to_string())
            );
        }
    }))
}

fn normalize_agent(agent: &str) -> Result<&str, AppError> {
    match agent {
        "cursor" | "opencode" => Ok(agent),
        _ => Err(AppError::NotFound(format!(
            "unknown agent pane: {agent} (use cursor or opencode)"
        ))),
    }
}

fn pane_index(agent: &str) -> &'static str {
    match agent {
        "opencode" => "1",
        _ => "0",
    }
}

async fn handle_socket(
    socket: WebSocket,
    session: String,
    window: String,
    pane: &'static str,
) -> anyhow::Result<()> {
    tmux::ensure_pane_zoomed(&session, &window, pane).await?;

    let target = format!("{session}:{window}");
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new("tmux");
    cmd.arg("attach-session");
    cmd.arg("-t");
    cmd.arg(&target);
    cmd.env_remove("GITHUB_TOKEN");
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let master = Arc::new(std::sync::Mutex::new(pair.master));

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let writer_task = tokio::spawn(async move {
        while let Some(data) = out_rx.recv().await {
            if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = ws_rx.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => break,
        };
        match msg {
            Message::Text(t) => {
                if let Ok(v) = serde_json::from_str::<WsClientMsg>(&t) {
                    if let (Some(cols), Some(rows)) = (v.cols, v.rows) {
                        if cols > 0 && rows > 0 {
                            if let Ok(m) = master.lock() {
                                let _ = m.resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                        }
                    }
                } else {
                    let _ = writer.write_all(t.as_bytes());
                    let _ = writer.flush();
                }
            }
            Message::Binary(b) => {
                let _ = writer.write_all(&b);
                let _ = writer.flush();
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }

    let _ = child.kill();
    writer_task.abort();
    Ok(())
}

#[derive(Debug, Deserialize)]
struct WsClientMsg {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    kind: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_normalize() {
        assert_eq!(normalize_agent("cursor").unwrap(), "cursor");
        assert_eq!(normalize_agent("opencode").unwrap(), "opencode");
        assert!(normalize_agent("claude").is_err());
    }

    #[test]
    fn pane_map() {
        assert_eq!(pane_index("cursor"), "0");
        assert_eq!(pane_index("opencode"), "1");
    }
}
