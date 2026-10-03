// Package node composes independent semantic owners and real provider adapters.
package node

import (
 "context"
 "errors"
 "net"
 "os"
 "sync"
 "floe/server/internal/authority"
 "floe/server/internal/credentials"
 "floe/server/internal/inference"
 "floe/server/internal/inference/providers"
 "floe/server/internal/integrations"
 "floe/server/internal/pairing"
 "floe/server/internal/trust"
 httptransport "floe/server/internal/transport/http"
)

type AuthRuntime interface {Action(context.Context,string)(any,error);providers.CodexClient}
type Console struct {mu sync.Mutex;directory,address string;vault credentials.Store;runtime AuthRuntime;trust *trust.Service;integrations *integrations.Service;pairing *pairing.Operations;gateway *inference.Service;handler *httptransport.Handler;state diskState;configUnavailable bool}
func New(directory,address string,vault credentials.Store,runtime AuthRuntime)(*Console,error){host,port,err:=net.SplitHostPort(address);if err!=nil||host!="127.0.0.1"||port==""{return nil,errors.New("console requires 127.0.0.1:port")};t,err:=trust.Open(directory);if err!=nil{return nil,err};state,err:=readState(directory);if err!=nil{return nil,err};model,err:=inference.NewService(t);if err!=nil{return nil,err};engine,err:=authority.New(authority.Options{Trust:t});if err!=nil{return nil,err};sources,err:=integrations.New(context.Background(),directory,t,vault,integrationFactories(directory,vault,os.Getenv),engine);if err!=nil{return nil,err};c:=&Console{directory:directory,address:address,vault:vault,runtime:runtime,trust:t,integrations:sources,state:state,gateway:model};c.pairing=pairing.NewOperations(t,vault,nil);if err=c.rebuild();err!=nil{sources.Close();return nil,err};c.handler=c.newHandler();return c,nil}
func(c *Console)RequiredSecurityError()error{if err:=c.trust.RequiredSecurityError();err!=nil{return err};if c.configUnavailable{return errors.New("inference persistence uncertain")};return nil}
func(c *Console)Close(){c.integrations.Close()}
func(c *Console)rebuild()error{if c.configUnavailable{return errors.New("configuration unavailable")};targets:=map[string]providers.ProviderTarget{};for id,t:=range c.state.Targets{targets[id]=t};for provider,p:=range c.state.Providers{for purpose:=range p.Purposes{targets[profileTargetID(provider,purpose)]=c.profileTarget(provider,purpose,p)}};registry,err:=providers.NewRegistry(targets,c.lookup,c.runtime);if err!=nil{return err};return c.gateway.Configure(inference.InferenceConfig{Routes:c.state.Routes},registry.Accounts(),registry)}
