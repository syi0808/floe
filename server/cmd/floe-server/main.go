package main

import (
	"context"
	"errors"
	"floe/server/internal/envfile"
	"floe/server/internal/node"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"
)

func main() {
	options, err := parseArgs(os.Args[1:])
	if err != nil {
		log.Fatalf("%v\nUsage: %s", err, usage())
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
	if options.printAdminToken {
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
	server, err := node.New(node.Config{Directory: directory, Address: address, DevelopmentQANoAuth: options.developmentQANoAuth})
	if err != nil {
		profileFailure(err, "Cannot start local server: verify the private profile, security state and loopback address")
	}
	defer server.Close()
	log.Printf("Local dashboard: http://%s/manage/", address)
	if options.developmentQANoAuth {
		log.Printf("!!! DEVELOPMENT QA MODE: DASHBOARD TOKEN AUTHENTICATION IS DISABLED at http://%s/manage/. Anyone with access to this local browser can administer this floe_dev profile. !!!", address)
	} else {
		log.Print("Administrator token is encrypted. Use this binary with --print-admin-token and the same FLOE_SERVER_DATA to retrieve it privately.")
	}
	serve(server, address)
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
