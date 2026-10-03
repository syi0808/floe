package inference

import (
 "context"
 "strings"
 "time"
 "floe/server/internal/trust"
)

type ProbeResult struct {TraceID string;ElapsedMS int64;Usage UsageObservation}
// ProbeTarget is an explicit operator diagnostic for any registered target,
// including one with no purpose route. Its input is fixed synthetic Agent data.
// Product inference still requires the exact observed purpose capability.
func(s *Service) ProbeTarget(ctx context.Context,operator trust.OperatorPrincipal,targetID string)(ProbeResult,error){
 if !ValidAlias(targetID){return ProbeResult{},Failure{Code:Validation}};if err:=s.trust.WithCurrentOperator(operator,func()error{return nil});err!=nil{return ProbeResult{},Failure{Code:Unauthorized}}
 s.mu.RLock();account:=s.accounts[targetID];executor:=s.executor;generation:=s.generation;denied:=s.unavailable;s.mu.RUnlock();if denied||executor==nil{return ProbeResult{},Failure{Code:ModelUnavailable}};if account==nil{return ProbeResult{},Failure{Code:NotFound}};if err:=account.Ready(ctx);err!=nil{return ProbeResult{},normalizeFailure(err)};identity:=account.ReplayIdentity();if identity==""{return ProbeResult{},Failure{Code:ProviderCredentialsUnavailable}}
 select{case s.active<-struct{}{}:defer func(){<-s.active}();default:return ProbeResult{},Failure{Code:ModelBusy}};if err:=ctx.Err();err!=nil{return ProbeResult{},err};if err:=s.trust.WithCurrentOperator(operator,func()error{return nil});err!=nil{return ProbeResult{},Failure{Code:Unauthorized}}
 s.mu.RLock();stale:=s.unavailable||s.generation!=generation;s.mu.RUnlock();if stale{return ProbeResult{},Failure{Code:CapabilityChanged}};if err:=account.Ready(ctx);err!=nil{return ProbeResult{},normalizeFailure(err)}
 text:="Confirm that this synthetic Agent connectivity check succeeded.";request:=AgentInvocation{Purpose:QuickResponse,AttemptID:trust.NewID(),DataClasses:[]string{"synthetic"},Instructions:"Reply with one brief plain-text confirmation. This is a Gateway operator diagnostic with no personal data or tools.",Input:AgentInput{Messages:[]Message{{Role:"user",Content:&text}},Tools:[]Tool{}},MaxOutputBytes:1024};target:=ResolvedModelTarget{targetID:targetID,accountIdentity:identity,generation:generation};trace:=newTraceID();started:=time.Now();bounded,cancel:=context.WithTimeout(ctx,40*time.Second);defer cancel();out,err:=executor.InvokeAgent(bounded,target,request)
 if err==nil{err=ValidateAgentResult(request,out)};if err==nil&&(len(out.Output)!=1||out.Output[0].Kind!="answer"||strings.TrimSpace(out.Output[0].Text)==""){err=Failure{Code:InvalidOutput}};if err==nil{err=s.trust.WithCurrentOperator(operator,func()error{return nil})};if err==nil{if e:=account.Ready(bounded);e!=nil{err=e}else if account.ReplayIdentity()!=identity{err=Failure{Code:IdentityMismatch}}};s.mu.RLock();changed:=s.unavailable||s.generation!=generation;s.mu.RUnlock();if err==nil&&changed{err=Failure{Code:CapabilityChanged}}
 if err!=nil{failure:=normalizeFailure(err);failure.TraceID=trace;failure.Dispatched=true;failure.Usage=out.Usage;s.audit.add(newAuditRecord(trace,"operator_probe",request.DataClasses,request,identity,string(failure.Code),nil,out.Usage,started));return ProbeResult{},failure};s.audit.add(newAuditRecord(trace,"operator_probe",request.DataClasses,request,identity,"completed",out.Output,out.Usage,started));return ProbeResult{TraceID:trace,ElapsedMS:time.Since(started).Milliseconds(),Usage:out.Usage},nil
}
