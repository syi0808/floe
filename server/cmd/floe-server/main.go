package main

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"floe/server/internal/codexauth"
	"floe/server/internal/connectors/gmail"
	"floe/server/internal/console"
	"floe/server/internal/credentials"
	"floe/server/internal/googleauth"
	"floe/server/internal/inference"
)

func main() {
	var handler http.Handler
	address := "127.0.0.1:8431"
	if os.Getenv("FLOE_INFERENCE_CONFIG") == "" || os.Getenv("FLOE_SERVER_DATA") != "" {
		directory := os.Getenv("FLOE_SERVER_DATA")
		if directory == "" {
			base, err := os.UserConfigDir()
			if err != nil {
				log.Fatal("Cannot locate server data directory")
			}
			directory = filepath.Join(base, "FloeServer")
		}
		if configured := os.Getenv("FLOE_SERVER_ADDRESS"); configured != "" {
			address = configured
		}
		vault := credentials.Keychain{}
		runtime := codexauth.New(vault)
		defer runtime.Close()
		management, err := console.New(directory, address, vault, runtime)
		if err != nil {
			log.Fatal("Cannot start local console: check private data directory and loopback address")
		}
		if clientID := os.Getenv("FLOE_GOOGLE_OAUTH_CLIENT_ID"); clientID != "" {
			gmailAuth, authError := googleauth.New(vault, googleauth.Config{ClientID: clientID, ClientSecret: os.Getenv("FLOE_GOOGLE_OAUTH_CLIENT_SECRET")})
			if authError != nil {
				log.Fatal("Cannot configure Google OAuth")
			}
			defer gmailAuth.Close()
			query := os.Getenv("FLOE_GMAIL_QUERY")
			if query == "" {
				query = "newer_than:30d -in:spam -in:trash"
			}
			gmailService, serviceError := gmail.NewService(filepath.Join(directory, "connectors", "gmail"), "primary", query, gmailAuth)
			if serviceError != nil {
				log.Fatal("Cannot initialize Gmail connector")
			}
			management.SetGmailAuth(gmailService)
			syncContext, stopSync := context.WithCancel(context.Background())
			defer stopSync()
			go func() { _ = gmailService.Run(syncContext, 5*time.Minute) }()
		}
		handler = management
		log.Printf("Local dashboard: http://%s/manage/", address)
		log.Printf("Administrator token file (keep private): %s", filepath.Join(directory, "admin-token"))
	} else {
		handler = legacyGateway()
	}
	serve(handler, address)
}

func legacyGateway() http.Handler {
	configFile, err := os.Open(os.Getenv("FLOE_INFERENCE_CONFIG"))
	if err != nil {
		log.Fatal("Set FLOE_INFERENCE_CONFIG to a gateway configuration file")
	}
	defer configFile.Close()
	decoder := json.NewDecoder(io.LimitReader(configFile, 65537))
	decoder.DisallowUnknownFields()
	var config inference.Config
	if decoder.Decode(&config) != nil || decoder.Decode(new(any)) != io.EOF {
		log.Fatal("Invalid inference configuration")
	}
	gateway, err := inference.New(config, os.Getenv("FLOE_INFERENCE_TOKEN"), os.Getenv)
	if err != nil {
		log.Fatal("Invalid inference configuration or missing credential")
	}
	return gateway
}

func serve(handler http.Handler, address string) {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	server := &http.Server{
		Addr: address, Handler: handler,
		ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second,
		WriteTimeout: 45 * time.Second, IdleTimeout: 30 * time.Second, MaxHeaderBytes: 8192,
		BaseContext: func(net.Listener) context.Context { return ctx },
	}
	shutdownComplete := make(chan struct{})
	go func() {
		defer close(shutdownComplete)
		<-ctx.Done()
		shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = server.Shutdown(shutdown)
	}()
	log.Printf("Floe inference gateway listening on %s", address)
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal("Inference gateway stopped unexpectedly")
	}
	<-shutdownComplete
}
