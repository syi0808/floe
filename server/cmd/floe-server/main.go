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
	"syscall"
	"time"

	"floe/server/internal/inference"
)

func main() {
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
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	server := &http.Server{
		Addr: "127.0.0.1:8431", Handler: gateway,
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
	log.Print("Floe inference gateway listening on 127.0.0.1:8431")
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal("Inference gateway stopped unexpectedly")
	}
	<-shutdownComplete
}
