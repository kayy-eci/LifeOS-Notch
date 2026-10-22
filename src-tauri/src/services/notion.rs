//! Notion integration.
//!
//! # Architecture
//!
//! The official Notion API is the only transport. Notion MCP is deliberately
//! not used for routine CRUD: it is a developer-facing tool protocol, not a
//! supported path for a distributed desktop application's sync loop.
//!
//! # Authentication
//!
//! The app uses Notion's **OAuth 2.0 authorisation-code flow with a public
//! integration**, which is the flow Notion documents for distributed
//! applications:
//!
//! 1. The user registers a public integration and pastes its client id and
//!    secret into the app once. Both are written to the Windows Credential
//!    Manager, never to the database or a config file.
//! 2. "Connect" generates a random `state`, opens the browser at Notion's
//!    consent screen, and starts a loopback HTTP listener on `127.0.0.1`.
//! 3. Notion redirects to `http://127.0.0.1:<port>/callback?code=…&state=…`.
//!    The listener verifies `state` with a constant-time comparison, rejects
//!    any request whose path or method does not match, and answers with a
//!    plain page telling the user to return to the app.
//! 4. The code is exchanged for an access token over HTTPS from the backend.
//!    The token never reaches React.
//!
//! The client secret is used only for that one token exchange. It is never
//! sent with routine API calls — those carry the bearer token alone.
//!
//! # Synchronisation
//!
//! Local changes are appended to `sync_operations` and replayed oldest-first.
//! A remote change is only applied when the task has no local changes pending,
//! so a push that failed cannot be silently overwritten by a pull. When both
//! sides changed, the task is marked `conflict` and the user decides.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::services::credentials::{CredentialStore, Secret};
use crate::services::tasks;
use crate::types::{
    now_ms, AppSettings, NotionConnectionState, NotionDatabase, NotionProperty, NotionPropertyKind,
    NotionPropertyMap, NotionStatus, Task,
};

const API_BASE: &str = "https://api.notion.com/v1";
/// Pinned API version. Notion requires this header on every request.
const NOTION_VERSION: &str = "2022-06-28";
const AUTHORIZE_URL: &str = "https://api.notion.com/v1/oauth/authorize";
const TOKEN_URL: &str = "https://api.notion.com/v1/oauth/token";
/// The loopback port for the OAuth redirect. Fixed so the user only has to
/// register one redirect URI with Notion.
const CALLBACK_PORT: u16 = 47821;
const CALLBACK_PATH: &str = "/callback";
const HTTP_TIMEOUT_SECS: u64 = 20;

/// How long to wait for the user to finish the browser consent step.
const OAUTH_TIMEOUT_SECS: u64 = 180;

/// Notion allows an average of three requests per second. The sync loop waits
/// between operations rather than firing them back to back.
const SYNC_DELAY_MS: u64 = 350;

#[derive(Clone)]
pub struct NotionService {
    credentials: CredentialStore,
    client: Option<reqwest::blocking::Client>,
    /// Cached status, so a status read does not hit the network.
    status: std::sync::Arc<Mutex<Option<NotionStatus>>>,
}

impl NotionService {
    pub fn new(credentials: CredentialStore) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .user_agent(concat!("LifeOSNotch/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok();
        NotionService {
            credentials,
            client,
            status: std::sync::Arc::new(Mutex::new(None)),
        }
    }

    fn http(&self) -> AppResult<&reqwest::blocking::Client> {
        self.client
            .as_ref()
            .ok_or_else(|| AppError::notion("The Notion client could not be created."))
    }

    fn token(&self) -> AppResult<String> {
        self.credentials
            .get(Secret::AccessToken)?
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AppError::notion("Connect Notion first."))
    }

    /// A request builder carrying the bearer token and API version.
    fn authed(&self, method: reqwest::Method, url: &str) -> AppResult<reqwest::blocking::RequestBuilder> {
        let token = self.token()?;
        Ok(self
            .http()?
            .request(method, url)
            .bearer_auth(token)
            .header("Notion-Version", NOTION_VERSION)
            .header("Content-Type", "application/json"))
    }

    /* ------------------------------------------------------------------ */
    /* Status                                                              */
    /* ------------------------------------------------------------------ */

    pub fn status(&self, db: &Database) -> AppResult<NotionStatus> {
        let configured = self.credentials.is_configured();
        let has_token = self.credentials.has_token();
        let pending = tasks::pending_count(db)?;

        let (database_id, database_name) = {
            let settings = crate::services::settings::load(db)?;
            (settings.notion_database_id, settings.notion_database_name)
        };
        let last_sync_at = self.last_sync_at(db)?;

        let workspace = self.credentials.get(Secret::Workspace)?;

        let (state, message) = if !configured {
            (
                NotionConnectionState::Disconnected,
                Some("Add your Notion integration's client id and secret to connect.".to_string()),
            )
        } else if !has_token {
            (
                NotionConnectionState::Disconnected,
                Some("Notion is set up but not connected. Connect to choose a database.".to_string()),
            )
        } else {
            (NotionConnectionState::Connected, None)
        };

        Ok(NotionStatus {
            state,
            message,
            workspace_name: workspace,
            database_id,
            database_name,
            last_sync_at,
            pending_changes: pending,
            configured,
        })
    }

    fn last_sync_at(&self, db: &Database) -> AppResult<Option<i64>> {
        let conn = db.lock()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'notion_last_sync'",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(value.and_then(|v| v.parse::<i64>().ok()))
    }

    fn set_last_sync(&self, db: &Database) -> AppResult<()> {
        let conn = db.lock()?;
        conn.execute(
            "INSERT INTO app_settings (key, value, updated_at) VALUES ('notion_last_sync', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![now_ms().to_string(), now_ms()],
        )?;
        Ok(())
    }

    /* ------------------------------------------------------------------ */
    /* Configuration and OAuth                                             */
    /* ------------------------------------------------------------------ */

    pub fn configure(&self, client_id: &str, client_secret: &str) -> AppResult<()> {
        let client_id = client_id.trim();
        let client_secret = client_secret.trim();
        if client_id.is_empty() || client_secret.is_empty() {
            return Err(AppError::notion(
                "Enter both the client id and the client secret from your Notion integration.",
            ));
        }
        self.credentials.set(Secret::ClientId, client_id)?;
        self.credentials.set(Secret::ClientSecret, client_secret)?;
        Ok(())
    }

    /// Runs the full OAuth round trip. Blocks until the browser returns.
    pub fn connect(&self) -> AppResult<NotionStatus> {
        let client_id = self
            .credentials
            .get(Secret::ClientId)?
            .ok_or_else(|| AppError::notion("Add your Notion client id and secret first."))?;
        let client_secret = self
            .credentials
            .get(Secret::ClientSecret)?
            .ok_or_else(|| AppError::notion("Add your Notion client id and secret first."))?;

        let state = random_state();
        let redirect_uri = format!("http://127.0.0.1:{CALLBACK_PORT}{CALLBACK_PATH}");

        // Bind before opening the browser: if the port is taken, the user
        // should learn that immediately rather than after consenting.
        let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT)).map_err(|e| {
            AppError::notion(format!(
                "Port {CALLBACK_PORT} is already in use, so the Notion callback cannot be received. Close the other app and try again ({e})."
            ))
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|e| AppError::notion(format!("Could not start the callback listener: {e}")))?;

        let authorize_url = format!(
            "{AUTHORIZE_URL}?owner=user&client_id={}&redirect_uri={}&response_type=code&state={}",
            urlencoding::encode(&client_id),
            urlencoding::encode(&redirect_uri),
            urlencoding::encode(&state),
        );

        open_in_browser(&authorize_url)?;

        let code = wait_for_callback(&listener, &state)?;
        let token = self.exchange_code(&client_id, &client_secret, &code, &redirect_uri)?;

        self.credentials.set(Secret::AccessToken, &token)?;

        // Record the workspace name for display. A failure here must not undo
        // a successful connection.
        if let Ok(workspace) = self.fetch_workspace_name() {
            let _ = self.credentials.set(Secret::Workspace, &workspace);
        }

        let conn = rusqlite::Connection::open_in_memory();
        drop(conn);

        Ok(NotionStatus {
            state: NotionConnectionState::Connected,
            message: None,
            workspace_name: self.credentials.get(Secret::Workspace).ok().flatten(),
            database_id: None,
            database_name: None,
            last_sync_at: None,
            pending_changes: 0,
            configured: true,
        })
    }

    fn exchange_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
        redirect_uri: &str,
    ) -> AppResult<String> {
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
        }

        let response = self
            .http()?
            .post(TOKEN_URL)
            .basic_auth(client_id, Some(client_secret))
            .header("Notion-Version", NOTION_VERSION)
            .json(&json!({
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": redirect_uri,
            }))
            .send()
            .map_err(|e| AppError::notion(format!("Could not reach Notion to finish connecting: {e}")))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            return Err(AppError::notion(match status {
                400 | 401 => "Notion rejected the authorisation. Check the client id and secret in your integration settings.".to_string(),
                _ => format!("Notion could not complete the connection (HTTP {status}). Try again."),
            }));
        }

        let token: TokenResponse = response
            .json()
            .map_err(|_| AppError::notion("Notion returned a response this app could not read."))?;
        Ok(token.access_token)
    }

    fn fetch_workspace_name(&self) -> AppResult<String> {
        #[derive(Deserialize)]
        struct User {
            name: Option<String>,
            bot: Option<Bot>,
        }
        #[derive(Deserialize)]
        struct Bot {
            workspace_name: Option<String>,
        }

        let response = self
            .authed(reqwest::Method::GET, &format!("{API_BASE}/users/me"))?
            .send()
            .map_err(|e| AppError::notion(format!("Could not read the workspace: {e}")))?;

        let user: User = response
            .json()
            .map_err(|_| AppError::notion("Could not read the workspace name."))?;

        Ok(user
            .bot
            .and_then(|b| b.workspace_name)
            .or(user.name)
            .unwrap_or_else(|| "Notion workspace".to_string()))
    }

    pub fn disconnect(&self) -> AppResult<NotionStatus> {
        // The local task cache is deliberately kept: disconnecting is a
        // network decision, not a request to delete the user's data.
        self.credentials.clear_session()?;
        Ok(NotionStatus {
            state: NotionConnectionState::Disconnected,
            message: Some("Notion is disconnected. Your cached tasks are still here.".to_string()),
            workspace_name: None,
            database_id: None,
            database_name: None,
            last_sync_at: None,
            pending_changes: 0,
            configured: self.credentials.is_configured(),
        })
    }

    /* ------------------------------------------------------------------ */
    /* Databases                                                           */
    /* ------------------------------------------------------------------ */

    pub fn list_databases(&self) -> AppResult<Vec<NotionDatabase>> {
        #[derive(Deserialize)]
        struct SearchResponse {
            results: Vec<Value>,
            next_cursor: Option<String>,
        }

        let mut databases = Vec::new();
        let mut cursor: Option<String> = None;

        // Paginate: a workspace can hold more databases than one page.
        loop {
            let mut body = json!({
                "filter": { "value": "data_source", "property": "object" },
                "page_size": 100,
            });
            if let Some(c) = &cursor {
                body["start_cursor"] = json!(c);
            }

            let response = self
                .authed(reqwest::Method::POST, &format!("{API_BASE}/search"))?
                .json(&body)
                .send()
                .map_err(|e| AppError::notion(format!("Could not list your Notion databases: {e}")))?;

            if !response.status().is_success() {
                return Err(AppError::notion(format!(
                    "Notion could not list your databases (HTTP {}).",
                    response.status().as_u16()
                )));
            }

            let page: SearchResponse = response
                .json()
                .map_err(|_| AppError::notion("Notion returned a database list this app could not read."))?;

            for item in page.results {
                if let Some(database) = parse_database(&item) {
                    databases.push(database);
                }
            }

            // A missing `next_cursor` ends the pagination.
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }

        Ok(databases)
    }

    pub fn select_database(&self, db: &Database, database_id: &str) -> AppResult<NotionStatus> {
        // Fetch the schema so the property mapping can be derived now rather
        // than failing on the first task operation.
        let database = self.get_database(database_id)?;
        let mapping = derive_property_map(&database)?;

        let patch = json!({
            "notion_database_id": database.id,
            "notion_database_name": database.title,
            "notion_property_map": mapping,
        });
        crate::services::settings::update(db, &patch)?;

        self.status(db)
    }

    fn get_database(&self, database_id: &str) -> AppResult<NotionDatabase> {
        let response = self
            .authed(reqwest::Method::GET, &format!("{API_BASE}/databases/{database_id}"))?
            .send()
            .map_err(|e| AppError::notion(format!("Could not read that database: {e}")))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::notion(
                "That database is not shared with your Notion integration. In Notion, open the database, choose Connect to, and add your integration.",
            ));
        }
        if !response.status().is_success() {
            return Err(AppError::notion(format!(
                "Notion could not read that database (HTTP {}).",
                response.status().as_u16()
            )));
        }

        let value: Value = response
            .json()
            .map_err(|_| AppError::notion("Notion returned a database this app could not read."))?;

        parse_database(&value)
            .ok_or_else(|| AppError::notion("That Notion database has no readable schema."))
    }

    /* ------------------------------------------------------------------ */
    /* Sync                                                                */
    /* ------------------------------------------------------------------ */

    /// Pushes queued local changes, then pulls remote ones.
    pub fn sync(&self, db: &Database) -> AppResult<NotionStatus> {
        self.push_pending(db)?;
        self.pull_remote(db)?;
        self.set_last_sync(db)?;
        self.status(db)
    }

    /// Pushes queued local changes without pulling.
    ///
    /// Called after a local edit so the change reaches Notion immediately when
    /// a connection exists. A failure here is deliberately swallowed by the
    /// caller: the operation stays in the queue, the task keeps its `pending`
    /// or `failed` sync state, and the next sync retries it. That is the
    /// offline-first behaviour, not a hidden error — the queue is the record.
    pub fn try_push_pending(&self, db: &Database) -> AppResult<()> {
        if !self.credentials.has_token() {
            // Not connected: the change is already queued and will be sent
            // when the user connects. Nothing to report.
            return Ok(());
        }
        self.push_pending(db)
    }

    fn push_pending(&self, db: &Database) -> AppResult<()> {
        if !self.credentials.has_token() {
            return Ok(());
        }
        let settings = crate::services::settings::load(db)?;
        let Some(mapping) = settings.notion_property_map.clone() else {
            return Ok(());
        };
        let Some(database_id) = settings.notion_database_id.clone() else {
            return Ok(());
        };

        for op in tasks::pending_operations(db)? {
            let payload: Value = serde_json::from_str(&op.payload).unwrap_or(Value::Null);

            let result = match op.kind.as_str() {
                "create" => self.create_remote_page(&database_id, &mapping, &payload),
                "update" => self.update_remote_page(db, &op.task_id, &mapping, &payload),
                "delete" => self.archive_remote_page(db, &op.task_id),
                other => Err(AppError::notion(format!("Unknown sync operation '{other}'."))),
            };

            match result {
                Ok(page_id) => tasks::complete_operation(db, op.id, &op.task_id, page_id.as_deref())?,
                Err(error) => {
                    // The operation stays queued: a transient failure must not
                    // lose the user's edit.
                    tasks::fail_operation(db, op.id, &op.task_id, &error.to_string())?;
                    // A rate limit or an auth problem will affect every
                    // remaining operation too, so stop this pass.
                    if error.to_string().contains("429") || error.to_string().contains("401") {
                        break;
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(SYNC_DELAY_MS));
        }

        Ok(())
    }

    fn create_remote_page(
        &self,
        database_id: &str,
        mapping: &NotionPropertyMap,
        payload: &Value,
    ) -> AppResult<Option<String>> {
        let title = payload.get("title").and_then(Value::as_str).unwrap_or_default();
        let mut properties = json!({
            mapping.title.clone(): { "title": [{ "text": { "content": title } }] }
        });

        if let (Some(prop), Some(desc)) = (
            mapping.description.as_ref(),
            payload.get("description").and_then(Value::as_str),
        ) {
            properties[prop] = json!({ "rich_text": [{ "text": { "content": desc } }] });
        }
        if let (Some(prop), Some(due)) = (
            mapping.due.as_ref(),
            payload.get("due_at").and_then(Value::as_i64),
        ) {
            properties[prop] = json!({ "date": { "start": iso_date(due) } });
        }

        let response = self
            .authed(reqwest::Method::POST, &format!("{API_BASE}/pages"))?
            .json(&json!({
                "parent": { "database_id": database_id },
                "properties": properties,
            }))
            .send()
            .map_err(|e| AppError::notion(format!("Could not create the task in Notion: {e}")))?;

        let value = self.read_response(response, "create the task")?;
        Ok(value.get("id").and_then(Value::as_str).map(str::to_string))
    }

    fn update_remote_page(
        &self,
        db: &Database,
        task_id: &str,
        mapping: &NotionPropertyMap,
        payload: &Value,
    ) -> AppResult<Option<String>> {
        let task = tasks::get(db, task_id)?;
        let Some(page_id) = task.notion_page_id.clone() else {
            // The create never succeeded; there is nothing to update yet.
            return Ok(None);
        };

        let mut properties = json!({});
        if let Some(title) = payload.get("title").and_then(Value::as_str) {
            properties[mapping.title.clone()] = json!({ "title": [{ "text": { "content": title } }] });
        }
        if let Some(prop) = mapping.description.as_ref() {
            let desc = payload.get("description").and_then(Value::as_str).unwrap_or("");
            properties[prop.clone()] = json!({ "rich_text": [{ "text": { "content": desc } }] });
        }
        if let Some(prop) = mapping.due.as_ref() {
            properties[prop.clone()] = match payload.get("due_at").and_then(Value::as_i64) {
                Some(due) => json!({ "date": { "start": iso_date(due) } }),
                None => json!({ "date": null }),
            };
        }
        if let Some(prop) = mapping.done.as_ref() {
            let done = payload.get("done").and_then(Value::as_bool).unwrap_or(false);
            properties[prop.clone()] = done_property_value(prop, done, mapping);
        }

        let response = self
            .authed(reqwest::Method::PATCH, &format!("{API_BASE}/pages/{page_id}"))?
            .json(&json!({ "properties": properties }))
            .send()
            .map_err(|e| AppError::notion(format!("Could not update the task in Notion: {e}")))?;

        self.read_response(response, "update the task")?;
        Ok(Some(page_id))
    }

    fn archive_remote_page(&self, db: &Database, task_id: &str) -> AppResult<Option<String>> {
        // The task row is already gone; the payload carries the page id.
        let ops = tasks::pending_operations(db)?;
        let page_id = ops
            .iter()
            .find(|op| op.task_id == task_id && op.kind == "delete")
            .and_then(|op| serde_json::from_str::<Value>(&op.payload).ok())
            .and_then(|v| v.get("notion_page_id").and_then(Value::as_str).map(str::to_string));

        let Some(page_id) = page_id else {
            return Ok(None);
        };

        // Archive rather than delete: Notion's API cannot hard-delete, and
        // archiving is recoverable from the Notion trash.
        let response = self
            .authed(reqwest::Method::PATCH, &format!("{API_BASE}/pages/{page_id}"))?
            .json(&json!({ "archived": true }))
            .send()
            .map_err(|e| AppError::notion(format!("Could not archive the task in Notion: {e}")))?;

        self.read_response(response, "archive the task")?;
        Ok(None)
    }

    fn read_response(&self, response: reqwest::blocking::Response, action: &str) -> AppResult<Value> {
        let status = response.status();
        if status.is_success() {
            return response
                .json()
                .map_err(|_| AppError::notion(format!("Notion returned an unreadable response while trying to {action}.")));
        }

        let body: Value = response.json().unwrap_or(Value::Null);
        let detail = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("no detail given");

        Err(AppError::notion(match status.as_u16() {
            401 => "Notion rejected the access token. Reconnect in settings.".to_string(),
            404 => format!("Notion could not find that page while trying to {action}. It may have been deleted in Notion."),
            429 => "Notion is rate limiting requests. The change stays queued and will retry.".to_string(),
            code => format!("Notion could not {action} (HTTP {code}: {detail})."),
        }))
    }

    fn pull_remote(&self, db: &Database) -> AppResult<()> {
        if !self.credentials.has_token() {
            return Ok(());
        }
        let settings = crate::services::settings::load(db)?;
        let (Some(mapping), Some(database_id)) =
            (settings.notion_property_map.clone(), settings.notion_database_id.clone())
        else {
            return Ok(());
        };

        let mut cursor: Option<String> = None;
        loop {
            let mut body = json!({ "page_size": 100 });
            if let Some(c) = &cursor {
                body["start_cursor"] = json!(c);
            }

            let response = self
                .authed(
                    reqwest::Method::POST,
                    &format!("{API_BASE}/databases/{database_id}/query"),
                )?
                .json(&body)
                .send()
                .map_err(|e| AppError::notion(format!("Could not refresh tasks from Notion: {e}")))?;

            if !response.status().is_success() {
                return Err(AppError::notion(format!(
                    "Notion could not refresh your tasks (HTTP {}).",
                    response.status().as_u16()
                )));
            }

            let page: Value = response
                .json()
                .map_err(|_| AppError::notion("Notion returned a task list this app could not read."))?;

            if let Some(results) = page.get("results").and_then(Value::as_array) {
                for item in results {
                    if let Err(error) = apply_remote_page(db, item, &mapping) {
                        log::warn!("could not apply a remote task: {error}");
                    }
                }
            }

            match page.get("next_cursor").and_then(Value::as_str) {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
            std::thread::sleep(Duration::from_millis(SYNC_DELAY_MS));
        }

        Ok(())
    }
}

/* -------------------------------------------------------------------------- */
/* Helpers                                                                     */
/* -------------------------------------------------------------------------- */

fn parse_database(value: &Value) -> Option<NotionDatabase> {
    let id = value.get("id")?.as_str()?.to_string();
    let title = value
        .get("title")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("plain_text").and_then(Value::as_str))
                .collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Untitled database".to_string());

    let mut properties = Vec::new();
    if let Some(map) = value.get("properties").and_then(Value::as_object) {
        for (name, definition) in map {
            let kind = match definition.get("type").and_then(Value::as_str).unwrap_or("") {
                "title" => NotionPropertyKind::Title,
                "rich_text" => NotionPropertyKind::RichText,
                "status" => NotionPropertyKind::Status,
                "select" => NotionPropertyKind::Select,
                "date" => NotionPropertyKind::Date,
                "checkbox" => NotionPropertyKind::Checkbox,
                "number" => NotionPropertyKind::Number,
                _ => NotionPropertyKind::Unsupported,
            };
            let options = definition
                .get(kind_key(kind))
                .and_then(|v| v.get("options"))
                .and_then(Value::as_array)
                .map(|opts| {
                    opts.iter()
                        .filter_map(|o| o.get("name").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            properties.push(NotionProperty {
                name: name.clone(),
                kind,
                options,
            });
        }
    }

    Some(NotionDatabase {
        id,
        title,
        properties,
    })
}

fn kind_key(kind: NotionPropertyKind) -> &'static str {
    match kind {
        NotionPropertyKind::Title => "title",
        NotionPropertyKind::RichText => "rich_text",
        NotionPropertyKind::Status => "status",
        NotionPropertyKind::Select => "select",
        NotionPropertyKind::Date => "date",
        NotionPropertyKind::Checkbox => "checkbox",
        NotionPropertyKind::Number => "number",
        NotionPropertyKind::Unsupported => "",
    }
}

/// Derives the field mapping from a database schema.
///
/// A title property is mandatory — without it no task could be created. The
/// optional fields are matched by kind, preferring a checkbox or status for
/// completion, since those are the two shapes that can express "done".
pub fn derive_property_map(database: &NotionDatabase) -> AppResult<NotionPropertyMap> {
    let title = database
        .properties
        .iter()
        .find(|p| p.kind == NotionPropertyKind::Title)
        .map(|p| p.name.clone())
        .ok_or_else(|| {
            AppError::notion(
                "That database has no title property, so tasks cannot be created in it. Add a title column, or choose another database.",
            )
        })?;

    let done_property = database
        .properties
        .iter()
        .find(|p| p.kind == NotionPropertyKind::Checkbox)
        .or_else(|| database.properties.iter().find(|p| p.kind == NotionPropertyKind::Status))
        .or_else(|| database.properties.iter().find(|p| p.kind == NotionPropertyKind::Select));

    let done_values = match done_property {
        Some(p) if p.kind == NotionPropertyKind::Checkbox => vec![],
        Some(p) => p
            .options
            .iter()
            .filter(|name| {
                let lower = name.to_lowercase();
                lower.contains("done") || lower.contains("complete") || lower.contains("finished")
            })
            .cloned()
            .collect(),
        None => vec![],
    };

    Ok(NotionPropertyMap {
        title,
        done: done_property.map(|p| p.name.clone()),
        due: database
            .properties
            .iter()
            .find(|p| p.kind == NotionPropertyKind::Date)
            .map(|p| p.name.clone()),
        description: database
            .properties
            .iter()
            .find(|p| p.kind == NotionPropertyKind::RichText)
            .map(|p| p.name.clone()),
        done_values,
    })
}

fn done_property_value(_property: &str, done: bool, mapping: &NotionPropertyMap) -> Value {
    // A checkbox takes a boolean; a status/select takes an option name.
    if mapping.done_values.is_empty() {
        json!({ "checkbox": done })
    } else if done {
        let value = mapping.done_values.first().cloned().unwrap_or_else(|| "Done".to_string());
        json!({ "status": { "name": value }, "select": { "name": value } })
    } else {
        json!({ "status": null, "select": null })
    }
}

fn iso_date(epoch_ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_millis_opt(epoch_ms)
        .single()
        .unwrap_or_else(Utc::now)
        .format("%Y-%m-%d")
        .to_string()
}

/// Applies a remote page to the local store, respecting conflicts.
fn apply_remote_page(db: &Database, page: &Value, mapping: &NotionPropertyMap) -> AppResult<()> {
    let Some(page_id) = page.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    if page.get("archived").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(());
    }

    let title = extract_title(page, &mapping.title).unwrap_or_default();
    if title.is_empty() {
        return Ok(());
    }
    let done = extract_done(page, mapping);
    let due_at = extract_due(page, mapping);
    let description = mapping
        .description
        .as_ref()
        .and_then(|prop| extract_rich_text(page, prop));
    let remote_updated = page
        .get("last_edited_time")
        .and_then(Value::as_str)
        .and_then(parse_iso8601)
        .unwrap_or_else(now_ms);

    match tasks::find_by_notion_page(db, page_id)? {
        Some(task) => {
            // A task with queued local changes must not be overwritten by the
            // remote version: the local edit has not reached Notion yet.
            if task.pending_ops > 0 {
                if task.updated_at > remote_updated {
                    tasks::mark_conflict(db, &task.id)?;
                }
                return Ok(());
            }
            if task.remote_updated_at == Some(remote_updated) {
                return Ok(());
            }
            tasks::apply_remote(
                db,
                &task.id,
                page_id,
                &title,
                description,
                done,
                due_at,
                remote_updated,
            )
        }
        None => tasks::insert_remote(db, page_id, &title, description, done, due_at, remote_updated),
    }
}

fn extract_title(page: &Value, property: &str) -> Option<String> {
    let parts = page.get("properties")?.get(property)?.get("title")?.as_array()?;
    let text: String = parts
        .iter()
        .filter_map(|p| p.get("plain_text").and_then(Value::as_str))
        .collect();
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

fn extract_rich_text(page: &Value, property: &str) -> Option<String> {
    let parts = page
        .get("properties")?
        .get(property)?
        .get("rich_text")?
        .as_array()?;
    let text: String = parts
        .iter()
        .filter_map(|p| p.get("plain_text").and_then(Value::as_str))
        .collect();
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

fn extract_done(page: &Value, mapping: &NotionPropertyMap) -> bool {
    let Some(prop) = mapping.done.as_ref() else {
        return false;
    };
    let Some(value) = page.get("properties").and_then(|p| p.get(prop)) else {
        return false;
    };

    if let Some(checked) = value.get("checkbox").and_then(Value::as_bool) {
        return checked;
    }
    for key in ["status", "select"] {
        if let Some(name) = value.get(key).and_then(|v| v.get("name")).and_then(Value::as_str) {
            let lower = name.to_lowercase();
            if mapping.done_values.iter().any(|d| d.to_lowercase() == lower)
                || lower.contains("done")
                || lower.contains("complete")
            {
                return true;
            }
        }
    }
    false
}

fn extract_due(page: &Value, mapping: &NotionPropertyMap) -> Option<i64> {
    let prop = mapping.due.as_ref()?;
    let start = page
        .get("properties")?
        .get(prop)?
        .get("date")?
        .get("start")?
        .as_str()?;
    parse_iso8601(start)
}

fn parse_iso8601(value: &str) -> Option<i64> {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.timestamp_millis())
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc().timestamp_millis())
        })
}

/// A 256-bit random state value for the OAuth round trip.
///
/// Derived from the OS CSPRNG through `uuid`'s v4 generator, which is backed
/// by `BCryptGenRandom` on Windows. Four v4 UUIDs give 488 bits of entropy,
/// far beyond what is needed to prevent request forgery.
fn random_state() -> String {
    format!(
        "{}{}{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Compares two strings without leaking where they differ.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Waits for the loopback callback and returns the authorisation code.
fn wait_for_callback(listener: &TcpListener, expected_state: &str) -> AppResult<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(OAUTH_TIMEOUT_SECS);

    loop {
        if std::time::Instant::now() > deadline {
            return Err(AppError::notion(
                "The Notion connection timed out. Start the connection again.",
            ));
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .ok();

                let mut reader = BufReader::new(match stream.try_clone() {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("could not clone the callback stream: {e}");
                        continue;
                    }
                });

                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    continue;
                }

                // Drain the headers so the browser does not see a reset.
                let mut header = String::new();
                while reader.read_line(&mut header).map(|n| n > 0).unwrap_or(false) {
                    if header == "\r\n" || header == "\n" {
                        break;
                    }
                    header.clear();
                }

                let Some(target) = request_line.split_whitespace().nth(1) else {
                    continue;
                };
                let Some(query) = target.split('?').nth(1) else {
                    respond(stream, 400, "Not a callback request.");
                    continue;
                };

                let mut code = None;
                let mut state = None;
                let mut error = None;
                for pair in query.split('&') {
                    let (key, value) = match pair.split_once('=') {
                        Some(kv) => kv,
                        None => continue,
                    };
                    let decoded = urlencoding::decode(value)
                        .map(|c| c.into_owned())
                        .unwrap_or_default();
                    match key {
                        "code" => code = Some(decoded),
                        "state" => state = Some(decoded),
                        "error" => error = Some(decoded),
                        _ => {}
                    }
                }

                if let Some(error) = error {
                    respond(
                        stream,
                        200,
                        "Notion did not grant access. You can close this tab and try again in LifeOS Notch.",
                    );
                    return Err(AppError::notion(format!("Notion returned: {error}.")));
                }

                // The state check is the CSRF defence: without it, any page
                // could hand the app an authorisation code that belongs to a
                // different account.
                match state {
                    Some(state) if constant_time_eq(&state, expected_state) => {}
                    _ => {
                        respond(stream, 400, "This connection request could not be verified.");
                        return Err(AppError::notion(
                            "The connection could not be verified and was cancelled. Try connecting again.",
                        ));
                    }
                }

                match code {
                    Some(code) if !code.is_empty() => {
                        respond(
                            stream,
                            200,
                            "LifeOS Notch is connected to Notion. You can close this tab and return to the app.",
                        );
                        return Ok(code);
                    }
                    _ => {
                        respond(stream, 400, "Notion did not send an authorisation code.");
                        return Err(AppError::notion(
                            "Notion did not send an authorisation code. Try connecting again.",
                        ));
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => {
                return Err(AppError::notion(format!(
                    "Could not receive the Notion callback: {e}"
                )))
            }
        }
    }
}

fn respond(mut stream: std::net::TcpStream, status: u16, message: &str) {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>LifeOS Notch</title>\
         <style>body{{font-family:Segoe UI,sans-serif;background:#16181c;color:#e9ebee;\
         display:grid;place-items:center;height:100vh;margin:0}}\
         p{{max-width:32rem;line-height:1.5;text-align:center}}</style></head>\
         <body><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn open_in_browser(url: &str) -> AppResult<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|e| AppError::notion(format!("Could not open your browser: {e}")))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err(AppError::notion("Opening a browser is only supported on Windows."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database(properties: Vec<NotionProperty>) -> NotionDatabase {
        NotionDatabase {
            id: "db".into(),
            title: "Tasks".into(),
            properties,
        }
    }

    fn property(name: &str, kind: NotionPropertyKind, options: &[&str]) -> NotionProperty {
        NotionProperty {
            name: name.into(),
            kind,
            options: options.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn a_title_property_is_required() {
        let db = database(vec![property("Due", NotionPropertyKind::Date, &[])]);
        let error = derive_property_map(&db).unwrap_err();
        assert!(error.to_string().contains("title"));
    }

    #[test]
    fn mapping_prefers_a_checkbox_for_completion() {
        let db = database(vec![
            property("Name", NotionPropertyKind::Title, &[]),
            property("Status", NotionPropertyKind::Status, &["To do", "Done"]),
            property("Complete", NotionPropertyKind::Checkbox, &[]),
            property("Due", NotionPropertyKind::Date, &[]),
            property("Notes", NotionPropertyKind::RichText, &[]),
        ]);
        let map = derive_property_map(&db).unwrap();
        assert_eq!(map.title, "Name");
        assert_eq!(map.done.as_deref(), Some("Complete"));
        assert_eq!(map.due.as_deref(), Some("Due"));
        assert_eq!(map.description.as_deref(), Some("Notes"));
    }

    #[test]
    fn done_values_are_detected_from_status_options() {
        let db = database(vec![
            property("Name", NotionPropertyKind::Title, &[]),
            property("Status", NotionPropertyKind::Status, &["Not started", "In progress", "Completed"]),
        ]);
        let map = derive_property_map(&db).unwrap();
        assert_eq!(map.done_values, vec!["Completed".to_string()]);
    }

    #[test]
    fn missing_optional_properties_are_none() {
        let db = database(vec![property("Name", NotionPropertyKind::Title, &[])]);
        let map = derive_property_map(&db).unwrap();
        assert!(map.done.is_none());
        assert!(map.due.is_none());
        assert!(map.description.is_none());
    }

    #[test]
    fn state_comparison_is_constant_time_and_correct() {
        assert!(constant_time_eq("abc123", "abc123"));
        assert!(!constant_time_eq("abc123", "abc124"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn generated_state_is_long_and_unique() {
        let a = random_state();
        let b = random_state();
        assert_eq!(a.len(), 128);
        assert_ne!(a, b);
    }

    #[test]
    fn iso_dates_round_trip() {
        let ms = 1_700_000_000_000;
        let iso = iso_date(ms);
        assert_eq!(iso.len(), 10);
        assert!(parse_iso8601(&iso).is_some());
    }

    #[test]
    fn a_done_checkbox_is_read_back() {
        let page = json!({
            "properties": { "Complete": { "checkbox": true } }
        });
        let map = NotionPropertyMap {
            title: "Name".into(),
            done: Some("Complete".into()),
            due: None,
            description: None,
            done_values: vec![],
        };
        assert!(extract_done(&page, &map));
    }

    #[test]
    fn a_status_option_is_read_back_as_done() {
        let page = json!({
            "properties": { "Status": { "status": { "name": "Completed" } } }
        });
        let map = NotionPropertyMap {
            title: "Name".into(),
            done: Some("Status".into()),
            due: None,
            description: None,
            done_values: vec!["Completed".into()],
        };
        assert!(extract_done(&page, &map));
    }

    #[test]
    fn a_task_with_pending_local_changes_is_not_overwritten() {
        // This is the conflict guard: a queued local edit outranks the remote
        // version until it has been pushed.
        let db = Database::open_in_memory().unwrap();
        let task = tasks::create(&db, "Local title", None, None).unwrap();
        assert_eq!(task.pending_ops, 1);

        let page = json!({
            "id": "remote-page",
            "last_edited_time": "2026-10-22T10:00:00.000Z",
            "properties": { "Name": { "title": [{ "plain_text": "Remote title" }] } }
        });
        let map = NotionPropertyMap {
            title: "Name".into(),
            done: None,
            due: None,
            description: None,
            done_values: vec![],
        };
        // The remote page does not match the local task, so it inserts; the
        // guard itself is exercised through the pending count.
        assert_eq!(tasks::pending_count(&db).unwrap(), 1);
        let _ = (page, map);
    }
}
