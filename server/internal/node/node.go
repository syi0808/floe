// Package node assembles semantic owners and real provider adapters.
package node

import (
    "context"
    "errors"
    "floe/server/internal/authority"
    "floe/server/internal/credentials"
    "floe/server/internal/inference"
    codexauth "floe/server/internal/inference/codex"
    "floe/server/internal/inference/providers"
    "floe/server/internal/integrations"
    "floe/server/internal/pairing"
    httptransport "floe/server/internal/transport/http"
    "floe/server/internal/trust"
    "net"
    "net/http"
    "os"
    "sync"
)

type Config struct {Directory,Address string}
type Node struct {
    handler *httptransport.Handler
    integrations *integrations.Service
    clients *trust.ClientAdministration
    runtime *codexauth.Runtime
    close sync.Once
    mu sync.Mutex
    active sync.WaitGroup
    closed bool
    ctx context.Context
    cancel context.CancelFunc
}
func New(config Config)(*Node,error){
    host,port,err:=net.SplitHostPort(config.Address)
    if err!=nil || host!="127.0.0.1" || port=="" {return nil,errors.New("node requires 127.0.0.1:port")}
    vault:=credentials.Keychain{}
    t,err:=trust.Open(config.Directory);if err!=nil{return nil,err}
    runtime:=codexauth.New(vault)
    failed:=true
    defer func(){if failed {runtime.Close()}}()
    model,err:=inference.NewService(t);if err!=nil{return nil,err}
    factory:=providers.NewFactory(vault.Get,runtime)
    configuration,err:=inference.OpenConfiguration(config.Directory,model,t,vault,factory);if err!=nil{return nil,err}
    engine,err:=authority.New(authority.Options{Trust:t});if err!=nil{return nil,err}
    sources,err:=integrations.New(context.Background(),config.Directory,t,vault,integrationFactories(config.Directory,vault,os.Getenv));if err!=nil{return nil,err}
    defer func(){if failed {sources.Close()}}()
    reader,err:=authority.NewSourceService(engine,t,sources,sources);if err!=nil{return nil,err}
    mirror,err:=authority.NewCalendarMirrorService(engine,t,sources,sources);if err!=nil{return nil,err}
    pairing:=pairing.NewOperations(t,vault,nil)
    clients:=trust.NewClientAdministration(t,sources,pairing)
    handler:=&httptransport.Handler{Address:config.Address,Trust:t,Pairing:pairing,Setup:sources,Integrations:sources,Sources:reader,Mirror:mirror,Configuration:configuration,Accounts:inference.NewAccountManagement(t,runtime),Clients:clients,Inference:&httptransport.InferenceHandler{Service:model,Trust:t,Address:config.Address}}
    failed=false
    ctx,cancel:=context.WithCancel(context.Background())
    return &Node{handler:handler,integrations:sources,clients:clients,runtime:runtime,ctx:ctx,cancel:cancel},nil
}
func (n *Node) ServeHTTP(w http.ResponseWriter,r *http.Request){
    n.mu.Lock()
    if n.closed {n.mu.Unlock();n.handler.ServeUnavailable(w);return}
    n.active.Add(1);n.mu.Unlock();defer n.active.Done()
    ctx,cancel:=context.WithCancel(r.Context());stop:=context.AfterFunc(n.ctx,cancel)
    defer stop();defer cancel()
    n.handler.ServeHTTP(w,r.WithContext(ctx))
}
func (n *Node) Close(){n.close.Do(func(){
    n.mu.Lock();n.closed=true;n.cancel();n.mu.Unlock()
    n.handler.Inference.Service.DenyConfiguration()
    n.active.Wait()
    n.clients.Close();n.integrations.Close();n.runtime.Close()
})}
