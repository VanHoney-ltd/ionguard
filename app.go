package main

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"

	wailsRuntime "github.com/wailsapp/wails/v2/pkg/runtime"
)

// Event represents a JSON event from the Rust core
type Event struct {
	EventType string  `json:"event_type"`
	IP        string  `json:"ip"`
	OldMAC    *string `json:"old_mac,omitempty"`
	NewMAC    *string `json:"new_mac,omitempty"`
	MAC       *string `json:"mac,omitempty"`
	Timestamp int64   `json:"timestamp"`
}

// Device represents a network device
type Device struct {
	IP        string `json:"ip"`
	MAC       string `json:"mac"`
	Iface     string `json:"iface"`
	State     string `json:"state"`
	LastSeen  int64  `json:"last_seen"`
}

// StatusResponse from STATUS command
type StatusResponse struct {
	Status   bool     `json:"status"`
	Running  bool     `json:"running"`
	Devices  int      `json:"devices"`
	Interval int      `json:"interval"`
}

// App struct
type App struct {
	ctx        context.Context
	coreCmd    *exec.Cmd
	coreStdin  io.WriteCloser
	coreReader *bufio.Reader
	mu         sync.Mutex
	running    bool
	devices    map[string]Device
	events     []Event
	devMu      sync.RWMutex
	evtMu      sync.RWMutex
}

// NewApp creates a new App
func NewApp() *App {
	return &App{
		devices: make(map[string]Device),
		events:  make([]Event, 0),
	}
}

func (a *App) startup(ctx context.Context) {
	a.ctx = ctx
	a.startCore()
}

func (a *App) shutdown(ctx context.Context) {
	a.stopCore()
}

func (a *App) startCore() {
	// Find the core binary: embedded near the executable, or in dev mode
	exePath, err := os.Executable()
	if err != nil {
		exePath = "."
	}
	exeDir := filepath.Dir(exePath)

	corePath := filepath.Join(exeDir, "ionguard-core")
	if _, err := os.Stat(corePath); err != nil {
		// Dev mode fallback
		corePath = filepath.Join(exeDir, "..", "core", "ionguard-core", "target", "release", "ionguard-core")
		if _, err := os.Stat(corePath); err != nil {
			corePath = filepath.Join(exeDir, "..", "..", "core", "ionguard-core", "target", "release", "ionguard-core")
		}
	}
	if runtime.GOOS == "windows" {
		corePath += ".exe"
	}

	cmd := exec.Command(corePath)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		println("Failed to get stdin pipe:", err.Error())
		return
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		println("Failed to get stdout pipe:", err.Error())
		return
	}
	cmd.Stderr = os.Stderr

	if err := cmd.Start(); err != nil {
		println("Failed to start core:", err.Error())
		return
	}

	a.mu.Lock()
	a.coreCmd = cmd
	a.coreStdin = stdin
	a.coreReader = bufio.NewReader(stdout)
	a.mu.Unlock()

	// Start reading events
	go a.readLoop()
}

func (a *App) stopCore() {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.coreStdin != nil {
		fmt.Fprintln(a.coreStdin, "STOP")
		a.coreStdin.Close()
	}
	if a.coreCmd != nil && a.coreCmd.Process != nil {
		a.coreCmd.Process.Kill()
		a.coreCmd.Wait()
	}
}

func (a *App) readLoop() {
	for {
		a.mu.Lock()
		reader := a.coreReader
		a.mu.Unlock()
		if reader == nil {
			return
		}

		line, err := reader.ReadString('\n')
		if err != nil {
			return
		}
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		// Try to parse as event first
		var evt Event
		if err := json.Unmarshal([]byte(line), &evt); err == nil && evt.EventType != "" {
			a.evtMu.Lock()
			a.events = append(a.events, evt)
			if len(a.events) > 500 {
				a.events = a.events[len(a.events)-500:]
			}
			a.evtMu.Unlock()

			wailsRuntime.EventsEmit(a.ctx, "ionguard:event", evt)
			continue
		}

		// Try status response
		var status StatusResponse
		if err := json.Unmarshal([]byte(line), &status); err == nil {
			wailsRuntime.EventsEmit(a.ctx, "ionguard:status", status)
		}
	}
}

func (a *App) sendCommand(cmd string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.coreStdin != nil {
		fmt.Fprintln(a.coreStdin, cmd)
	}
}

// StartMonitor begins monitoring the LAN
func (a *App) StartMonitor() error {
	a.mu.Lock()
	a.running = true
	a.mu.Unlock()
	a.sendCommand("START")
	return nil
}

// StopMonitor pauses monitoring
func (a *App) StopMonitor() error {
	a.mu.Lock()
	a.running = false
	a.mu.Unlock()
	a.sendCommand("STOP")
	return nil
}

// IsMonitoring returns true if monitoring is active
func (a *App) IsMonitoring() bool {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.running
}

// SetInterval changes the polling interval in milliseconds
func (a *App) SetInterval(ms int) error {
	a.sendCommand(fmt.Sprintf("INTERVAL %d", max(ms, 500)))
	return nil
}

// IgnoreDevices sets IPs to ignore (comma-separated)
func (a *App) IgnoreDevices(ips string) error {
	a.sendCommand(fmt.Sprintf("IGNORE %s", ips))
	return nil
}

// TriggerSweep forces an active sweep
func (a *App) TriggerSweep() error {
	a.sendCommand("SWEEP")
	return nil
}

// GetStatus requests a status update from the core
func (a *App) GetStatus() error {
	a.sendCommand("STATUS")
	return nil
}

// GetEvents returns recent events
func (a *App) GetEvents() []Event {
	a.evtMu.RLock()
	defer a.evtMu.RUnlock()
	out := make([]Event, len(a.events))
	copy(out, a.events)
	return out
}

// GetDevices returns the current device map
func (a *App) GetDevices() map[string]Device {
	a.devMu.RLock()
	defer a.devMu.RUnlock()
	out := make(map[string]Device, len(a.devices))
	for k, v := range a.devices {
		out[k] = v
	}
	return out
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
