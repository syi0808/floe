package node

import (
 "context"
 "net/http"
 "floe/server/internal/connections"
 "floe/server/internal/operation"
 "floe/server/internal/trust"
 httptransport "floe/server/internal/transport/http"
)
func(c *Console)ServeHTTP(w http.ResponseWriter,r *http.Request){c.handler.ServeHTTP(w,r)}
func(c *Console)newHandler()*httptransport.Handler{return &httptransport.Handler{Address:c.address,Trust:c.trust,Pairing:c.pairing,Setup:c.integrations,Inference:&httptransport.InferenceHandler{Service:c.gateway,Trust:c.trust,Address:c.address},Authenticate:c.authenticate,Management:httptransport.Management{State:c.managementState,Codex:c.codexAction,Route:c.updateRoute,Target:c.updateTarget,Provider:c.updateProvider,Test:c.testTarget,DeleteClient:c.deleteClient,DeleteTarget:c.deleteTarget}}}
func(c *Console)authenticate(ctx context.Context,token string)(httptransport.Client,operation.Result){p,err:=c.trust.AuthenticateBearer(ctx,token);if err!=nil{return httptransport.Client{},trust.Result(err)};source,err:=c.integrations.SourceService(p);if err!=nil{return httptransport.Client{},trust.Result(err)};return httptransport.Client{Principal:p,Sources:source,List:func(ctx context.Context)operation.Result{return c.integrations.List(ctx,p)},Connectors:httptransport.ConnectorOperations{Catalog:func()operation.Result{return c.integrations.Catalog(ctx,p)},Start:func(ctx context.Context,id string,in connections.ConnectRequest)operation.Result{return c.integrations.Start(ctx,p,id,in)},Attempt:func(ctx context.Context,id,attempt string)operation.Result{return c.integrations.Poll(ctx,p,id,attempt)},Cancel:func(ctx context.Context,id,attempt string,in connections.CancelSetupRequest)operation.Result{return c.integrations.Cancel(ctx,p,id,attempt,in)},Update:func(id string,in connections.ScopeRequest)operation.Result{return c.integrations.UpdateScope(ctx,p,id,in)},Disconnect:func(ctx context.Context,id string,in connections.DisconnectRequest)operation.Result{return c.integrations.Disconnect(ctx,p,id,in)}}},operation.Accept(nil)}
