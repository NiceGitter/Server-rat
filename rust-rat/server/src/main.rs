use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tokio::process::Command;
use uuid::Uuid;

// Структура для хранения информации о клиентах
#[derive(Debug, Clone, Serialize)]
struct Client {
    id: Uuid,
    hostname: String,
    ip_address: String,
    os_info: String,
    last_seen: DateTime<Utc>,
    is_online: bool,
}

// Структура для регистрации нового клиента
#[derive(Debug, Deserialize)]
struct RegisterClientRequest {
    hostname: String,
    os_info: String,
}

// Структура для выполнения команды
#[derive(Debug, Deserialize)]
struct ExecuteCommandRequest {
    command: String,
}

// Структура для результата выполнения команды
#[derive(Debug, Serialize)]
struct CommandResult {
    output: String,
    error: Option<String>,
    exit_code: i32,
}

// Структура для скриншота
#[derive(Debug, Serialize, Deserialize)]
struct Screenshot {
    data: String, // base64 encoded
    timestamp: DateTime<Utc>,
}

// Состояние приложения
struct AppState {
    clients: RwLock<HashMap<Uuid, Client>>,
    command_results: RwLock<HashMap<Uuid, Vec<CommandResult>>>,
    screenshots: RwLock<HashMap<Uuid, Vec<Screenshot>>>,
}

#[tokio::main]
async fn main() {
    // Инициализация логгера
    tracing_subscriber::fmt::init();

    // Создаем состояние приложения
    let app_state = Arc::new(AppState {
        clients: RwLock::new(HashMap::new()),
        command_results: RwLock::new(HashMap::new()),
        screenshots: RwLock::new(HashMap::new()),
    });

    // Настраиваем роутер
    let app = Router::new()
        .route("/api/clients", get(list_clients))
        .route("/api/clients/:id", get(get_client))
        .route("/api/clients/register", post(register_client))
        .route("/api/clients/:id/ping", post(ping_client))
        .route("/api/clients/:id/execute", post(execute_command))
        .route("/api/clients/:id/screenshot", post(upload_screenshot))
        .route("/api/clients/:id/screenshots", get(list_screenshots))
        .route("/api/clients/:id/commands", get(list_command_results))
        .with_state(app_state);

    // Запускаем сервер
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Запуск сервера по адресу: {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Обработчики API

// Получить список всех клиентов
async fn list_clients(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let clients = state.clients.read().unwrap();
    let clients_vec: Vec<Client> = clients.values().cloned().collect();
    
    (StatusCode::OK, Json(clients_vec))
}

// Получить информацию о конкретном клиенте
async fn get_client(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let clients = state.clients.read().unwrap();
    
    if let Some(client) = clients.get(&id) {
        (StatusCode::OK, Json(client.clone()))
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Клиент не найден" })))
    }
}

// Зарегистрировать нового клиента
async fn register_client(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegisterClientRequest>,
) -> impl IntoResponse {
    let client_id = Uuid::new_v4();
    
    let client = Client {
        id: client_id,
        hostname: request.hostname,
        ip_address: "unknown".to_string(), // В реальном приложении будет использоваться IP из запроса
        os_info: request.os_info,
        last_seen: Utc::now(),
        is_online: true,
    };
    
    {
        let mut clients = state.clients.write().unwrap();
        clients.insert(client_id, client.clone());
    }
    
    (StatusCode::CREATED, Json(client))
}

// Обновить статус клиента (ping)
async fn ping_client(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut clients = state.clients.write().unwrap();
    
    if let Some(client) = clients.get_mut(&id) {
        client.last_seen = Utc::now();
        client.is_online = true;
        
        (StatusCode::OK, Json(client.clone()))
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Клиент не найден" })))
    }
}

// Выполнить команду на клиенте
// В реальности это было бы реализовано через WebSocket или другое постоянное соединение
async fn execute_command(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(request): Json<ExecuteCommandRequest>,
) -> impl IntoResponse {
    // Проверяем существование клиента
    {
        let clients = state.clients.read().unwrap();
        if !clients.contains_key(&id) {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Клиент не найден" })));
        }
    }
    
    // В реальной программе здесь был бы код для отправки команды клиенту
    // и получения результата
    
    // Эмулируем выполнение команды на сервере
    let output = Command::new("sh")
        .arg("-c")
        .arg(&request.command)
        .output()
        .await;
    
    let result = match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = if output.stderr.is_empty() {
                None
            } else {
                Some(String::from_utf8_lossy(&output.stderr).to_string())
            };
            
            CommandResult {
                output: stdout,
                error: stderr,
                exit_code: output.status.code().unwrap_or(-1),
            }
        },
        Err(e) => {
            CommandResult {
                output: String::new(),
                error: Some(e.to_string()),
                exit_code: -1,
            }
        }
    };
    
    // Сохраняем результат команды
    {
        let mut command_results = state.command_results.write().unwrap();
        command_results.entry(id).or_insert_with(Vec::new).push(result.clone());
    }
    
    (StatusCode::OK, Json(result))
}

// Загрузить скриншот от клиента
async fn upload_screenshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(screenshot): Json<Screenshot>,
) -> impl IntoResponse {
    // Проверяем существование клиента
    {
        let clients = state.clients.read().unwrap();
        if !clients.contains_key(&id) {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Клиент не найден" })));
        }
    }
    
    // Сохраняем скриншот
    {
        let mut screenshots = state.screenshots.write().unwrap();
        screenshots.entry(id).or_insert_with(Vec::new).push(screenshot);
    }
    
    (StatusCode::OK, Json(serde_json::json!({ "status": "success" })))
}

// Получить список скриншотов клиента
async fn list_screenshots(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let screenshots = state.screenshots.read().unwrap();
    
    if let Some(client_screenshots) = screenshots.get(&id) {
        (StatusCode::OK, Json(client_screenshots.clone()))
    } else {
        (StatusCode::OK, Json(Vec::<Screenshot>::new()))
    }
}

// Получить историю выполнения команд
async fn list_command_results(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let command_results = state.command_results.read().unwrap();
    
    if let Some(client_results) = command_results.get(&id) {
        (StatusCode::OK, Json(client_results.clone()))
    } else {
        (StatusCode::OK, Json(Vec::<CommandResult>::new()))
    }
}
