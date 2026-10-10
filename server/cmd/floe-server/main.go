package main

import (
	"context"
	"errors"
	"floe/server/internal/adapters/storage/privatefiles"
	"floe/server/internal/envfile"
	"floe/server/internal/modelcatalog"
	"floe/server/internal/node"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"
)

func main() {
	printToken := len(os.Args) == 2 && os.Args[1] == "--print-admin-token"
	catalogCommand, catalogPath, commandOK := parseCatalogCommand(os.Args[1:])
	if !commandOK || len(os.Args) > 1 && !printToken && catalogCommand == "" {
		log.Fatal("Usage: floe-server [--print-admin-token | --validate-model-catalog FILE | --install-model-catalog FILE | --rollback-model-catalog]")
	}
	if catalogCommand != "" {
		directory := ""
		if catalogCommand != "validate" {
			directory = dataDirectoryFromProcessEnvironment()
		}
		runCatalogCommand(catalogCommand, catalogPath, directory)
		return
	}
	if envfile.Load() != nil {
		log.Fatal("Cannot load environment file")
	}
	directory := os.Getenv("FLOE_SERVER_DATA")
	if directory == "" {
		base, err := os.UserConfigDir()
		if err != nil {
			log.Fatal("Cannot locate server data directory")
		}
		directory = node.DefaultDataDirectory(base)
	}
	if printToken {
		token, err := node.AdministratorToken(context.Background(), directory)
		if err != nil {
			profileFailure(err, "Cannot read administrator token from the existing encrypted profile")
		}
		fmt.Println(token)
		return
	}
	address := os.Getenv("FLOE_SERVER_ADDRESS")
	if address == "" {
		address = node.DefaultAddress()
	}
	refreshInterval, err := modelcatalog.ParseRefreshInterval(os.Getenv("FLOE_MODEL_CATALOG_REFRESH_INTERVAL"))
	if err != nil {
		log.Fatal(err)
	}
	server, err := node.New(node.Config{Directory: directory, Address: address, ModelCatalogRefreshInterval: refreshInterval})
	if err != nil {
		profileFailure(err, "Cannot start local server: verify the private profile, security state and loopback address")
	}
	defer server.Close()
	log.Printf("Local dashboard: http://%s/manage/", address)
	log.Print("Administrator token is encrypted. Use this binary with --print-admin-token and the same FLOE_SERVER_DATA to retrieve it privately.")
	serve(server, address)
}

func parseCatalogCommand(args []string) (command, path string, ok bool) {
	switch {
	case len(args) == 2 && args[0] == "--validate-model-catalog":
		return "validate", args[1], true
	case len(args) == 2 && args[0] == "--install-model-catalog":
		return "install", args[1], true
	case len(args) == 1 && args[0] == "--rollback-model-catalog":
		return "rollback", "", true
	case len(args) == 0 || len(args) == 1 && args[0] == "--print-admin-token":
		return "", "", true
	default:
		return "", "", false
	}
}

// Catalog file commands intentionally use only process environment and local
// files. They do not load the provider-related .env file or access credentials.
func dataDirectoryFromProcessEnvironment() string {
	if directory := os.Getenv("FLOE_SERVER_DATA"); directory != "" {
		return directory
	}
	base, err := os.UserConfigDir()
	if err != nil {
		log.Fatal("Cannot locate server data directory")
	}
	return node.DefaultDataDirectory(base)
}

func runCatalogCommand(command, sourcePath, directory string) {
	if command == "validate" {
		if _, err := modelcatalog.ValidateFile(sourcePath); err != nil {
			log.Fatalf("Model catalog validation failed: %s", catalogError(err))
		}
		fmt.Println("Model catalog is valid.")
		return
	}
	if err := storage.PrivateDirectory(directory); err != nil {
		log.Fatal("Catalog updates require an existing private server profile")
	}
	target := filepath.Join(directory, modelcatalog.FileName)
	switch command {
	case "install":
		if err := modelcatalog.InstallFile(target, sourcePath); err != nil {
			log.Fatalf("Model catalog installation failed: %s", catalogError(err))
		}
		fmt.Println("Model catalog installed; a running server reloads it within its configured interval.")
	case "rollback":
		if err := modelcatalog.Rollback(target); err != nil {
			log.Fatalf("Model catalog rollback failed: %s", catalogError(err))
		}
		fmt.Println("Model catalog rolled back; a running server reloads it within its configured interval.")
	default:
		log.Fatal("Unknown catalog command")
	}
}

func catalogError(err error) string {
	switch {
	case errors.Is(err, modelcatalog.ErrInvalidCatalog):
		return "invalid or oversized data"
	case errors.Is(err, modelcatalog.ErrStaleCatalog):
		return "revision is stale"
	case errors.Is(err, modelcatalog.ErrCatalogLockUnavailable):
		return "catalog writer lock is unavailable"
	case errors.Is(err, modelcatalog.ErrCatalogPersistence):
		return "last-good snapshot could not be saved; inspect current, previous, last-good, and pending rollback files before retrying"
	case errors.Is(err, modelcatalog.ErrUnsupportedCatalogPlatform):
		return "catalog file commands are unsupported on this platform"
	case errors.Is(err, modelcatalog.ErrPendingRollback):
		return "a prior rollback needs recovery"
	case errors.Is(err, os.ErrNotExist):
		return "required local file is missing"
	case storage.IsIndeterminate(err):
		return "durability is uncertain; inspect current, previous, last-good, and pending rollback files before retrying"
	default:
		return "local file operation unavailable"
	}
}

func serve(handler http.Handler, address string) {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	server := &http.Server{Addr: address, Handler: handler, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, WriteTimeout: 45 * time.Second, IdleTimeout: 30 * time.Second, MaxHeaderBytes: 8192, BaseContext: func(net.Listener) context.Context { return ctx }}
	done := make(chan struct{})
	go func() {
		defer close(done)
		<-ctx.Done()
		shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = server.Shutdown(shutdown)
	}()
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal("Local server stopped unexpectedly")
	}
	<-done
}

func profileFailure(err error, message string) {
	var issue *node.StorageFailure
	if errors.As(err, &issue) {
		log.Fatalf("%s (category=%s; operation=%s; stage=%s). Existing files and keys were preserved.", message, issue.Code(), issue.Operation(), issue.Stage())
	}
	log.Fatal(message)
}
