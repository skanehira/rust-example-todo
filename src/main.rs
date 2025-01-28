use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, TryLockError};
use tracing::{info, instrument, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
}

struct AppError {
    status: StatusCode,
    message: String,
}

impl<T> From<TryLockError<T>> for AppError {
    fn from(err: TryLockError<T>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.to_string(),
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (self.status, self.message).into_response()
    }
}

type HttpResult<T> = Result<T, AppError>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_file(true) // ファイル名を表示
        .with_line_number(true) // 行番号を表示
        .with_level(true) // ログレベルを表示
        .with_max_level(Level::TRACE)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let conn = Connection::open_in_memory()?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL
        )",
        (),
    )?;

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };

    let app = Router::new()
        .route("/users", post(create_user))
        .route("/users", get(get_user))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[instrument(skip(state))]
async fn get_user(State(state): State<AppState>) -> HttpResult<Json<Vec<User>>> {
    info!("get user");

    let db = state.db.try_lock()?;
    let mut stmt = db.prepare("SELECT id, name FROM users")?;

    let iter = stmt.query_map([], |row| {
        Ok(User {
            id: row.get(0)?,
            username: row.get(1)?,
        })
    })?;

    let mut users = Vec::with_capacity(iter.size_hint().0);
    for person in iter {
        users.push(person?);
    }

    Ok(Json(users))
}

#[instrument(skip(state), fields(payload = ?payload))]
async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUser>,
) -> HttpResult<Json<User>> {
    info!("create user");

    let user = User {
        id: 1337,
        username: payload.username,
    };

    let db = state.db.try_lock()?;

    db.execute(
        "INSERT INTO users (id, name) VALUES (?, ?)",
        (&user.id, &user.username),
    )
    .map_err(AppError::from)?;

    Ok(Json(user))
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    username: String,
}

#[derive(Debug, Serialize)]
struct User {
    id: u64,
    username: String,
}
