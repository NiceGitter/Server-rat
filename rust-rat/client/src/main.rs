use chrono::{DateTime, Utc};
use eframe::{
    egui::{self, Context, Ui},
    epi::{App, Frame},
    NativeOptions,
};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::runtime::Runtime;
use uuid::Uuid;

// Структуры данных, синхронизированные с сервером

#[derive(Debug, Clone, Deserialize)]
struct RemoteClient {
    id: Uuid,
    hostname: String,
    ip_address: String,
    os_info: String,
    last_seen: DateTime<Utc>,
    is_online: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct CommandResult {
    output: String,
    error: Option<String>,
    exit_code: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct Screenshot {
    data: String, // base64 encoded
    timestamp: DateTime<Utc>,
}

// Состояние приложения
struct RatClientApp {
    // HTTP клиент для связи с сервером
    http_client: HttpClient,
    // Адрес сервера
    server_url: String,
    // Токио рантайм для асинхронных запросов
    runtime: Runtime,
    // Список клиентов
    clients: Arc<Mutex<Vec<RemoteClient>>>,
    // Выбранный клиент
    selected_client_idx: Option<usize>,
    // Команда для выполнения
    command_input: String,
    // Результаты команд
    command_results: Arc<Mutex<HashMap<Uuid, Vec<CommandResult>>>>,
    // Скриншоты
    screenshots: Arc<Mutex<HashMap<Uuid, Vec<Screenshot>>>>,
    // Текущий просматриваемый скриншот
    current_screenshot_idx: Option<usize>,
    // Статус последней операции
    status_message: String,
    // Интервал обновления (в секундах)
    refresh_interval: u64,
    // Время последнего обновления
    last_refresh: DateTime<Utc>,
}

impl Default for RatClientApp {
    fn default() -> Self {
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http_client,
            server_url: "http://localhost:3000".to_string(),
            runtime,
            clients: Arc::new(Mutex::new(Vec::new())),
            selected_client_idx: None,
            command_input: String::new(),
            command_results: Arc::new(Mutex::new(HashMap::new())),
            screenshots: Arc::new(Mutex::new(HashMap::new())),
            current_screenshot_idx: None,
            status_message: "Готов к работе".to_string(),
            refresh_interval: 5,
            last_refresh: Utc::now(),
        }
    }
}

impl App for RatClientApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // Проверка необходимости обновления данных
        let now = Utc::now();
        if (now - self.last_refresh).num_seconds() as u64 >= self.refresh_interval {
            self.refresh_clients();
            self.last_refresh = now;
        }

        // Верхняя панель с настройками
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Файл", |ui| {
                    if ui.button("Выход").clicked() {
                        _frame.quit();
                    }
                });
                ui.menu_button("Настройки", |ui| {
                    ui.label("Адрес сервера:");
                    ui.text_edit_singleline(&mut self.server_url);
                    ui.label("Интервал обновления (с):");
                    ui.add(egui::Slider::new(&mut self.refresh_interval, 1..=60));
                });
                ui.menu_button("Справка", |ui| {
                    if ui.button("О программе").clicked() {
                        self.status_message = "RAT Client v0.1.0".to_string();
                    }
                });
            });
        });

        // Статусная строка
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_message);
            });
        });

        // Левая панель со списком клиентов
        egui::SidePanel::left("clients_panel").show(ctx, |ui| {
            ui.heading("Клиенты");
            if ui.button("Обновить").clicked() {
                self.refresh_clients();
            }
            ui.separator();
            
            let clients = self.clients.lock().unwrap();
            let mut selected_idx = self.selected_client_idx;
            
            for (idx, client) in clients.iter().enumerate() {
                let text = format!("{} [{}]", client.hostname, if client.is_online { "онлайн" } else { "оффлайн" });
                let selected = selected_idx == Some(idx);
                
                if ui.selectable_label(selected, text).clicked() {
                    selected_idx = Some(idx);
                    if let Some(idx) = selected_idx {
                        let client_id = clients[idx].id;
                        self.load_client_data(client_id);
                    }
                }
            }
            
            self.selected_client_idx = selected_idx;
        });

        // Основная область с информацией о выбранном клиенте
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.selected_client_idx {
                let clients = self.clients.lock().unwrap();
                if idx < clients.len() {
                    let client = &clients[idx];
                    
                    ui.heading(&client.hostname);
                    ui.label(format!("IP адрес: {}", client.ip_address));
                    ui.label(format!("ОС: {}", client.os_info));
                    ui.label(format!("Статус: {}", if client.is_online { "онлайн" } else { "оффлайн" }));
                    ui.label(format!("Последняя активность: {}", client.last_seen));
                    
                    ui.separator();
                    
                    // Вкладки для разных функций
                    egui::TabBar::new(&mut [
                        ("Терминал", self.tab_terminal(ui, client.id)),
                        ("Скриншоты", self.tab_screenshots(ui, client.id)),
                        ("История команд", self.tab_command_history(ui, client.id)),
                    ])
                    .ui(ui);
                }
            } else {
                ui.heading("Выберите клиента из списка слева");
            }
        });
    }

    fn name(&self) -> &str {
        "RAT Client"
    }
}

impl RatClientApp {
    // Вкладка с терминалом
    fn tab_terminal(&mut self, ui: &mut Ui, client_id: Uuid) -> bool {
        ui.heading("Выполнение команд");
        
        ui.horizontal(|ui| {
            ui.label("Команда:");
            ui.text_edit_singleline(&mut self.command_input);
            
            if ui.button("Выполнить").clicked() && !self.command_input.is_empty() {
                self.execute_command(client_id, &self.command_input);
            }
        });
        
        ui.separator();
        
        // Показываем последний результат выполнения команды, если есть
        let results = self.command_results.lock().unwrap();
        if let Some(client_results) = results.get(&client_id) {
            if let Some(last_result) = client_results.last() {
                ui.heading("Результат:");
                ui.monospace(&last_result.output);
                
                if let Some(ref error) = last_result.error {
                    ui.heading("Ошибки:");
                    ui.monospace(error);
                }
                
                ui.label(format!("Код выхода: {}", last_result.exit_code));
            }
        }
        
        true
    }
    
    // Вкладка со скриншотами
    fn tab_screenshots(&mut self, ui: &mut Ui, client_id: Uuid) -> bool {
        ui.heading("Скриншоты");
        
        if ui.button("Сделать скриншот").clicked() {
            self.request_screenshot(client_id);
        }
        
        ui.separator();
        
        let screenshots = self.screenshots.lock().unwrap();
        if let Some(client_screenshots) = screenshots.get(&client_id) {
            if client_screenshots.is_empty() {
                ui.label("Нет доступных скриншотов");
            } else {
                // Навигация по скриншотам
                ui.horizontal(|ui| {
                    if ui.button("Предыдущий").clicked() {
                        if let Some(idx) = self.current_screenshot_idx {
                            if idx > 0 {
                                self.current_screenshot_idx = Some(idx - 1);
                            }
                        } else {
                            self.current_screenshot_idx = Some(0);
                        }
                    }
                    
                    let total = client_screenshots.len();
                    let current = self.current_screenshot_idx.unwrap_or(0) + 1;
                    ui.label(format!("{}/{}", current, total));
                    
                    if ui.button("Следующий").clicked() {
                        if let Some(idx) = self.current_screenshot_idx {
                            if idx < client_screenshots.len() - 1 {
                                self.current_screenshot_idx = Some(idx + 1);
                            }
                        } else {
                            self.current_screenshot_idx = Some(0);
                        }
                    }
                });
                
                // Отображение текущего скриншота
                let idx = self.current_screenshot_idx.unwrap_or(0);
                if idx < client_screenshots.len() {
                    let screenshot = &client_screenshots[idx];
                    ui.label(format!("Время: {}", screenshot.timestamp));
                    
                    // В реальном приложении здесь был бы код для декодирования 
                    // base64 и отображения изображения
                    ui.label("Предпросмотр скриншота недоступен в этой версии");
                }
            }
        } else {
            ui.label("Нет доступных скриншотов");
        }
        
        true
    }
    
    // Вкладка с историей команд
    fn tab_command_history(&mut self, ui: &mut Ui, client_id: Uuid) -> bool {
        ui.heading("История команд");
        
        let results = self.command_results.lock().unwrap();
        if let Some(client_results) = results.get(&client_id) {
            if client_results.is_empty() {
                ui.label("Нет истории команд");
            } else {
                for (i, result) in client_results.iter().enumerate().rev() {
                    ui.collapsing(format!("Команда #{}", i+1), |ui| {
                        ui.monospace(&result.output);
                        if let Some(ref error) = result.error {
                            ui.monospace(error);
                        }
                        ui.label(format!("Код выхода: {}", result.exit_code));
                    });
                }
            }
        } else {
            ui.label("Нет истории команд");
        }
        
        true
    }
    
    // Обновление списка клиентов
    fn refresh_clients(&mut self) {
        let clients_clone = self.clients.clone();
        let http_client = self.http_client.clone();
        let server_url = self.server_url.clone();
        let status_message = &mut self.status_message;
        
        self.runtime.block_on(async move {
            match http_client.get(&format!("{}/api/clients", server_url)).send().await {
                Ok(response) => {
                    match response.json::<Vec<RemoteClient>>().await {
                        Ok(remote_clients) => {
                            *clients_clone.lock().unwrap() = remote_clients;
                            *status_message = "Список клиентов обновлен".to_string();
                        },
                        Err(e) => {
                            *status_message = format!("Ошибка парсинга ответа: {}", e);
                        }
                    }
                },
                Err(e) => {
                    *status_message = format!("Ошибка соединения: {}", e);
                }
            }
        });
    }
    
    // Загрузка данных выбранного клиента (команды и скриншоты)
    fn load_client_data(&mut self, client_id: Uuid) {
        self.load_command_history(client_id);
        self.load_screenshots(client_id);
    }
    
    // Загрузка истории команд
    fn load_command_history(&mut self, client_id: Uuid) {
        let command_results = self.command_results.clone();
        let http_client = self.http_client.clone();
        let server_url = self.server_url.clone();
        let status_message = &mut self.status_message;
        
        self.runtime.block_on(async move {
            match http_client.get(&format!("{}/api/clients/{}/commands", server_url, client_id)).send().await {
                Ok(response) => {
                    match response.json::<Vec<CommandResult>>().await {
                        Ok(results) => {
                            command_results.lock().unwrap().insert(client_id, results);
                        },
                        Err(e) => {
                            *status_message = format!("Ошибка парсинга истории команд: {}", e);
                        }
                    }
                },
                Err(e) => {
                    *status_message = format!("Ошибка загрузки истории команд: {}", e);
                }
            }
        });
    }
    
    // Загрузка скриншотов
    fn load_screenshots(&mut self, client_id: Uuid) {
        let screenshots = self.screenshots.clone();
        let http_client = self.http_client.clone();
        let server_url = self.server_url.clone();
        let status_message = &mut self.status_message;
        
        self.runtime.block_on(async move {
            match http_client.get(&format!("{}/api/clients/{}/screenshots", server_url, client_id)).send().await {
                Ok(response) => {
                    match response.json::<Vec<Screenshot>>().await {
                        Ok(client_screenshots) => {
                            screenshots.lock().unwrap().insert(client_id, client_screenshots);
                        },
                        Err(e) => {
                            *status_message = format!("Ошибка парсинга скриншотов: {}", e);
                        }
                    }
                },
                Err(e) => {
                    *status_message = format!("Ошибка загрузки скриншотов: {}", e);
                }
            }
        });
    }
    
    // Выполнение команды на клиенте
    fn execute_command(&mut self, client_id: Uuid, command: &str) {
        let command_results = self.command_results.clone();
        let http_client = self.http_client.clone();
        let server_url = self.server_url.clone();
        let status_message = &mut self.status_message;
        let command = command.to_string();
        
        self.runtime.block_on(async move {
            let command_request = serde_json::json!({
                "command": command
            });
            
            match http_client.post(&format!("{}/api/clients/{}/execute", server_url, client_id))
                .json(&command_request)
                .send().await {
                Ok(response) => {
                    match response.json::<CommandResult>().await {
                        Ok(result) => {
                            let mut results = command_results.lock().unwrap();
                            results.entry(client_id).or_insert_with(Vec::new).push(result);
                            *status_message = "Команда выполнена".to_string();
                        },
                        Err(e) => {
                            *status_message = format!("Ошибка парсинга результата команды: {}", e);
                        }
                    }
                },
                Err(e) => {
                    *status_message = format!("Ошибка выполнения команды: {}", e);
                }
            }
        });
    }
    
    // Запрос скриншота
    fn request_screenshot(&mut self, client_id: Uuid) {
        let screenshots = self.screenshots.clone();
        let http_client = self.http_client.clone();
        let server_url = self.server_url.clone();
        let status_message = &mut self.status_message;
        
        self.runtime.block_on(async move {
            // В реальном приложении здесь был бы код для запроса скриншота от клиента
            // Пока просто добавляем фиктивный скриншот для демонстрации
            
            let screenshot = serde_json::json!({
                "data": "fake_base64_data",
                "timestamp": chrono::Utc::now()
            });
            
            match http_client.post(&format!("{}/api/clients/{}/screenshot", server_url, client_id))
                .json(&screenshot)
                .send().await {
                Ok(_) => {
                    *status_message = "Запрос скриншота отправлен".to_string();
                    
                    // Обновляем список скриншотов
                    match http_client.get(&format!("{}/api/clients/{}/screenshots", server_url, client_id)).send().await {
                        Ok(response) => {
                            match response.json::<Vec<Screenshot>>().await {
                                Ok(client_screenshots) => {
                                    screenshots.lock().unwrap().insert(client_id, client_screenshots);
                                },
                                Err(e) => {
                                    *status_message = format!("Ошибка парсинга скриншотов: {}", e);
                                }
                            }
                        },
                        Err(e) => {
                            *status_message = format!("Ошибка загрузки скриншотов: {}", e);
                        }
                    }
                },
                Err(e) => {
                    *status_message = format!("Ошибка запроса скриншота: {}", e);
                }
            }
        });
    }
}

fn main() {
    let native_options = NativeOptions::default();
    eframe::run_native(
        Box::new(RatClientApp::default()),
        native_options,
    );
}
