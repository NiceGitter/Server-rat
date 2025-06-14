# Руководство по развертыванию на виртуальном Ubuntu сервере

В этом руководстве описывается процесс развертывания серверной части RAT на виртуальном сервере Ubuntu.

## Требования

- Виртуальный сервер с Ubuntu 20.04 или новее
- Sudo права
- Открытый порт 3000 (или другой настроенный порт) на брандмауэре

## Шаги по установке

### 1. Установка зависимостей

Подключитесь к вашему серверу по SSH и выполните следующие команды для установки необходимых зависимостей:

```bash
# Обновление пакетов
sudo apt update
sudo apt upgrade -y

# Установка инструментов сборки и Rust
sudo apt install -y build-essential pkg-config libssl-dev curl git

# Установка Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Клонирование и сборка проекта

```bash
# Клонирование репозитория (замените на ваш репозиторий)
git clone https://github.com/your-username/rust-rat.git
cd rust-rat

# Сборка серверной части
cd server
cargo build --release
```

### 3. Настройка запуска сервера как службы

Создайте файл службы systemd:

```bash
sudo nano /etc/systemd/system/rat-server.service
```

Добавьте следующее содержимое (замените пути на соответствующие вашей системе):

```ini
[Unit]
Description=RAT Server Service
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/rust-rat/server
ExecStart=/home/ubuntu/rust-rat/server/target/release/rat-server
Restart=always
RestartSec=5
StandardOutput=syslog
StandardError=syslog
SyslogIdentifier=rat-server

[Install]
WantedBy=multi-user.target
```

Запустите и включите службу:

```bash
sudo systemctl daemon-reload
sudo systemctl enable rat-server
sudo systemctl start rat-server
```

Проверка статуса:

```bash
sudo systemctl status rat-server
```

### 4. Настройка брандмауэра

Если на вашем сервере установлен UFW, разрешите входящие подключения на порт 3000:

```bash
sudo ufw allow 3000
```

### 5. Проверка работоспособности сервера

Вы можете проверить, что сервер успешно запущен с помощью следующей команды:

```bash
curl http://localhost:3000/api/clients
```

Это должно вернуть пустой массив `[]`, если нет подключенных клиентов.

## Безопасность

Для повышения безопасности рекомендуется:

1. Настроить HTTPS с помощью Let's Encrypt и Nginx как обратного прокси
2. Настроить аутентификацию для API
3. Ограничить доступ по IP
4. Использовать защищенную VPN сеть

## Настройка Nginx как обратного прокси (опционально)

Установите Nginx:

```bash
sudo apt install -y nginx
```

Создайте конфигурационный файл:

```bash
sudo nano /etc/nginx/sites-available/rat-server
```

Добавьте следующее содержимое:

```nginx
server {
    listen 80;
    server_name your-domain.com;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
    }
}
```

Активируйте конфигурацию:

```bash
sudo ln -s /etc/nginx/sites-available/rat-server /etc/nginx/sites-enabled/
sudo systemctl restart nginx
```

Для настройки HTTPS с Let's Encrypt:

```bash
sudo apt install -y certbot python3-certbot-nginx
sudo certbot --nginx -d your-domain.com
```

## Обновление

Чтобы обновить сервер до новой версии:

```bash
cd /path/to/rust-rat
git pull
cd server
cargo build --release
sudo systemctl restart rat-server
``` 