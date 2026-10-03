package node

import (
 "encoding/json"
 "errors"
 "os"
 "path/filepath"
 "floe/server/internal/inference"
 "floe/server/internal/inference/providers"
 "floe/server/internal/storage"
 "floe/server/internal/trust"
)

type diskState struct {SchemaVersion int `json:"schema_version"`;Targets map[string]providers.ProviderTarget `json:"targets"`;Routes map[inference.Purpose]inference.PurposeRoute `json:"routes"`;Providers map[string]providerProfile `json:"providers"`}
type providerProfile struct {BaseURL string `json:"base_url"`;APIKeyEnv string `json:"api_key_env,omitempty"`;Purposes map[string]inference.PurposeModel `json:"purposes"`}
func readState(directory string)(diskState,error){st:=diskState{1,map[string]providers.ProviderTarget{},map[inference.Purpose]inference.PurposeRoute{},map[string]providerProfile{}};data,err:=storage.ReadPrivate(filepath.Join(directory,"inference.json"),65536);if os.IsNotExist(err){return st,nil};if err!=nil||trust.DecodeStrict(data,&st,65536,32)!=nil||st.SchemaVersion!=1||st.Targets==nil||st.Routes==nil||st.Providers==nil||len(st.Targets)>32||len(st.Providers)>3||inference.ValidateConfig(inference.InferenceConfig{Routes:st.Routes})!=nil{return st,errors.New("inference configuration unavailable")};for _,t:=range st.Targets{if providers.ValidateTarget(t)!=nil{return st,errors.New("invalid inference target")}};for _,p:=range st.Providers{if p.Purposes==nil||len(p.Purposes)>3{return st,errors.New("invalid provider configuration")};for purpose:=range p.Purposes{if !inference.ValidPurpose(purpose){return st,errors.New("invalid purpose")}}};return st,nil}
func cloneState(st diskState)diskState{out:=diskState{st.SchemaVersion,map[string]providers.ProviderTarget{},map[inference.Purpose]inference.PurposeRoute{},map[string]providerProfile{}};for k,v:=range st.Targets{out.Targets[k]=v};for k,v:=range st.Routes{out.Routes[k]=v};for k,v:=range st.Providers{copy:=v;copy.Purposes=map[string]inference.PurposeModel{};for k,c:=range v.Purposes{copy.Purposes[k]=c};out.Providers[k]=copy};return out}
func(c *Console)save(st diskState)error{data,err:=json.Marshal(st);if err!=nil{return err};err=storage.WritePrivate(filepath.Join(c.directory,"inference.json"),data);if storage.IsIndeterminate(err){c.configUnavailable=true;c.gateway.DenyConfiguration()};return err}
func randomToken()string{return trust.Token()}
func(c *Console)lookup(name string)(string,error){return c.vault.Get(name)}
