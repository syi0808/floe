package main

import (
 "context"
 "errors"
 "log"
 "net"
 "net/http"
 "os"
 "os/signal"
 "path/filepath"
 "syscall"
 "time"
 "floe/server/internal/node"
 codexauth "floe/server/internal/inference/codex"
 "floe/server/internal/credentials"
 "floe/server/internal/envfile"
)
func main(){if envfile.Load()!=nil{log.Fatal("Cannot load environment file")};directory:=os.Getenv("FLOE_SERVER_DATA");if directory==""{base,err:=os.UserConfigDir();if err!=nil{log.Fatal("Cannot locate server data directory")};directory=filepath.Join(base,"FloeServer")};address:=os.Getenv("FLOE_SERVER_ADDRESS");if address==""{address="127.0.0.1:8431"};vault:=credentials.Keychain{};runtime:=codexauth.New(vault);defer runtime.Close();server,err:=node.NewLocal(node.LocalConfig{Directory:directory,Address:address,Vault:vault,Runtime:runtime});if err!=nil{log.Fatal("Cannot start local server: verify the private profile, security state and loopback address")};defer server.Close();log.Printf("Local dashboard: http://%s/manage/",address);log.Printf("Administrator token file (keep private): %s",filepath.Join(directory,"admin-token"));serve(server,address)}
func serve(handler http.Handler,address string){ctx,stop:=signal.NotifyContext(context.Background(),os.Interrupt,syscall.SIGTERM);defer stop();server:=&http.Server{Addr:address,Handler:handler,ReadHeaderTimeout:5*time.Second,ReadTimeout:10*time.Second,WriteTimeout:45*time.Second,IdleTimeout:30*time.Second,MaxHeaderBytes:8192,BaseContext:func(net.Listener)context.Context{return ctx}};done:=make(chan struct{});go func(){defer close(done);<-ctx.Done();shutdown,cancel:=context.WithTimeout(context.Background(),5*time.Second);defer cancel();_ = server.Shutdown(shutdown)}();if err:=server.ListenAndServe();err!=nil&&!errors.Is(err,http.ErrServerClosed){log.Fatal("Local server stopped unexpectedly")};<-done}
