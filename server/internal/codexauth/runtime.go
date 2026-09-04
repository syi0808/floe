package codexauth

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"
)

var unavailable = errors.New("Codex authentication unavailable")

type message struct {
	ID     json.RawMessage `json:"id"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params"`
	Result json.RawMessage `json:"result"`
	Error  json.RawMessage `json:"error"`
}

type Runtime struct {
	operation                    sync.Mutex
	mu                           sync.Mutex
	write                        sync.Mutex
	directory, executable        string
	command                      *exec.Cmd
	stdin                        io.WriteCloser
	done                         chan struct{}
	next                         int64
	pending                      map[int64]chan message
	loginID, authURL, loginState string
	expires                      time.Time
	completionID                 string
	completionSuccess            bool
	expiry                       *time.Timer
}

func New(directory string) *Runtime {
	path, _ := exec.LookPath("codex")
	return &Runtime{directory: directory, executable: path}
}

func (runtime *Runtime) start(ctx context.Context) error {
	if runtime.command != nil {
		select {
		case <-runtime.done:
			runtime.command = nil
		default:
			return nil
		}
	}
	if runtime.executable == "" {
		return unavailable
	}
	home := filepath.Join(runtime.directory, "codex-home")
	work := filepath.Join(runtime.directory, "codex-work")
	if os.MkdirAll(home, 0700) != nil || os.MkdirAll(work, 0700) != nil {
		return unavailable
	}
	command := exec.Command(runtime.executable, "-c", `cli_auth_credentials_store="keyring"`, "app-server", "--listen", "stdio://")
	command.Dir = work
	command.Env = []string{"PATH=" + os.Getenv("PATH"), "HOME=" + os.Getenv("HOME"), "CODEX_HOME=" + home, "TMPDIR=" + os.TempDir()}
	command.Stderr = io.Discard
	stdin, err := command.StdinPipe()
	if err != nil {
		return unavailable
	}
	stdout, err := command.StdoutPipe()
	if err != nil {
		stdin.Close()
		return unavailable
	}
	if command.Start() != nil {
		stdin.Close()
		return unavailable
	}
	runtime.mu.Lock()
	runtime.command, runtime.stdin, runtime.done = command, stdin, make(chan struct{})
	runtime.pending = map[int64]chan message{}
	runtime.loginID, runtime.authURL, runtime.loginState = "", "", "disconnected"
	runtime.mu.Unlock()
	go runtime.read(stdout, command, runtime.done)
	_, err = runtime.call(ctx, "initialize", map[string]any{"clientInfo": map[string]string{"name": "floe_auth", "title": "Floe connections", "version": "0.1.0"}, "capabilities": map[string]bool{"experimentalApi": false}})
	if err != nil {
		runtime.stop()
		return err
	}
	if runtime.send(map[string]any{"method": "initialized", "params": map[string]any{}}) != nil {
		runtime.stop()
		return unavailable
	}
	return nil
}

func (runtime *Runtime) read(stdout io.Reader, command *exec.Cmd, done chan struct{}) {
	defer close(done)
	defer command.Wait()
	defer command.Process.Kill()
	scanner := bufio.NewScanner(stdout)
	scanner.Buffer(make([]byte, 4096), 1<<20)
	for scanner.Scan() {
		var event message
		if json.Unmarshal(scanner.Bytes(), &event) != nil {
			return
		}
		if event.Method != "" && len(event.ID) > 0 {
			_ = runtime.send(map[string]any{"id": event.ID, "error": map[string]any{"code": -32601, "message": "Floe authentication adapter does not execute tools"}})
			continue
		}
		runtime.mu.Lock()
		if event.Method == "account/login/completed" {
			var completion struct {
				LoginID string `json:"loginId"`
				Success bool   `json:"success"`
			}
			if json.Unmarshal(event.Params, &completion) == nil {
				runtime.completionID, runtime.completionSuccess = completion.LoginID, completion.Success
				if completion.LoginID != runtime.loginID {
					runtime.mu.Unlock()
					continue
				}
				runtime.loginID, runtime.authURL = "", ""
				runtime.loginState = "reconnect_required"
				if completion.Success {
					runtime.loginState = "connected"
				}
			}
		} else if len(event.ID) > 0 {
			var identifier int64
			if json.Unmarshal(event.ID, &identifier) == nil {
				if reply := runtime.pending[identifier]; reply != nil {
					reply <- event
					delete(runtime.pending, identifier)
				}
			}
		}
		runtime.mu.Unlock()
	}
}

func (runtime *Runtime) send(value any) error {
	runtime.write.Lock()
	defer runtime.write.Unlock()
	return json.NewEncoder(runtime.stdin).Encode(value)
}

func (runtime *Runtime) call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	runtime.mu.Lock()
	runtime.next++
	identifier := runtime.next
	reply := make(chan message, 1)
	runtime.pending[identifier] = reply
	runtime.mu.Unlock()
	defer func() { runtime.mu.Lock(); delete(runtime.pending, identifier); runtime.mu.Unlock() }()
	if runtime.send(map[string]any{"id": identifier, "method": method, "params": params}) != nil {
		return nil, unavailable
	}
	select {
	case event := <-reply:
		if len(event.Error) > 0 && string(event.Error) != "null" {
			return nil, unavailable
		}
		return event.Result, nil
	case <-ctx.Done():
		return nil, unavailable
	case <-runtime.done:
		return nil, unavailable
	}
}

func (runtime *Runtime) Action(ctx context.Context, action string) (any, error) {
	if action != "status" && action != "login" && action != "cancel" && action != "logout" {
		return nil, unavailable
	}
	if !runtime.operation.TryLock() {
		return nil, unavailable
	}
	defer runtime.operation.Unlock()
	if ctx.Err() != nil {
		return nil, unavailable
	}
	if err := runtime.start(ctx); err != nil {
		return nil, err
	}
	runtime.mu.Lock()
	loginID, expired := runtime.loginID, time.Now().After(runtime.expires)
	runtime.mu.Unlock()
	if loginID != "" && (expired || action == "cancel" || action == "logout") {
		if _, err := runtime.call(ctx, "account/login/cancel", map[string]string{"loginId": loginID}); err != nil {
			runtime.stop()
			return nil, err
		}
		runtime.mu.Lock()
		runtime.loginID, runtime.authURL, runtime.loginState = "", "", "disconnected"
		runtime.mu.Unlock()
		loginID = ""
	}
	if action == "login" && loginID == "" {
		body, err := runtime.call(ctx, "account/login/start", map[string]any{"type": "chatgpt"})
		if err != nil {
			runtime.stop()
			return nil, err
		}
		var result struct {
			LoginID string `json:"loginId"`
			AuthURL string `json:"authUrl"`
		}
		if json.Unmarshal(body, &result) != nil || result.LoginID == "" || !validAuthURL(result.AuthURL) {
			runtime.stop()
			return nil, unavailable
		}
		runtime.mu.Lock()
		runtime.loginID, runtime.authURL, runtime.loginState, runtime.expires = result.LoginID, result.AuthURL, "pending", time.Now().Add(5*time.Minute)
		if runtime.completionID == result.LoginID {
			runtime.loginID, runtime.authURL, runtime.loginState = "", "", "reconnect_required"
			if runtime.completionSuccess {
				runtime.loginState = "connected"
			}
		}
		runtime.mu.Unlock()
		if runtime.expiry != nil {
			runtime.expiry.Stop()
		}
		runtime.expiry = time.AfterFunc(5*time.Minute, func() { runtime.expire(result.LoginID) })
	}
	if action == "logout" {
		if _, err := runtime.call(ctx, "account/logout", nil); err != nil {
			runtime.stop()
			return nil, err
		}
		runtime.mu.Lock()
		runtime.loginState = "disconnected"
		runtime.mu.Unlock()
	}
	body, err := runtime.call(ctx, "account/read", map[string]bool{"refreshToken": false})
	if err != nil {
		runtime.stop()
		return nil, err
	}
	var account struct {
		Account *struct {
			Type string `json:"type"`
		} `json:"account"`
	}
	if json.Unmarshal(body, &account) != nil {
		return nil, unavailable
	}
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.loginID == "" && account.Account != nil {
		runtime.loginState = "connected"
	}
	if runtime.loginID == "" && account.Account == nil && runtime.loginState == "connected" {
		runtime.loginState = "reconnect_required"
	}
	return map[string]any{"status": runtime.loginState, "auth_url": runtime.authURL, "inference_enabled": false}, nil
}

func validAuthURL(value string) bool {
	parsed, err := url.Parse(value)
	return err == nil && parsed.Scheme == "https" && parsed.User == nil && parsed.Port() == "" && (parsed.Hostname() == "auth.openai.com" || parsed.Hostname() == "chatgpt.com")
}

func (runtime *Runtime) stop() {
	if runtime.expiry != nil {
		runtime.expiry.Stop()
	}
	if runtime.command != nil {
		_ = runtime.command.Process.Kill()
		<-runtime.done
		runtime.command = nil
	}
}

func (runtime *Runtime) expire(identifier string) {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.mu.Lock()
	active := runtime.command != nil && runtime.loginID == identifier
	runtime.mu.Unlock()
	if !active {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_, err := runtime.call(ctx, "account/login/cancel", map[string]string{"loginId": identifier})
	if err != nil {
		runtime.stop()
	}
	runtime.mu.Lock()
	runtime.loginID, runtime.authURL, runtime.loginState = "", "", "disconnected"
	runtime.mu.Unlock()
}

func (runtime *Runtime) Close() {
	runtime.operation.Lock()
	defer runtime.operation.Unlock()
	runtime.stop()
}
