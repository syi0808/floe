package lifecycle

import (
 "context"
 "encoding/json"
 "errors"
 "time"
 "floe/server/internal/integrations"
 "floe/server/internal/trust"
 "floe/server/internal/views"
)
// RegisterReaders freezes exact connection-scoped adapters. No Reader chooses a provider.
func RegisterReaders(runtime integrations.Runtime,config integrations.RuntimeConfig,descriptor integrations.Descriptor)integrations.Runtime{
 runtime.Descriptor=descriptor;runtime.Readers=map[views.ID]views.Reader{}
 for _,d:=range descriptor.Views{id:=views.ID(d.ID);if id!=views.Calendar&&id!=views.Communication&&id!=views.WorkContext&&id!=views.Logistics{continue};captured:=runtime
  runtime.Readers[id]=views.ReaderFunc(func(ctx context.Context,request views.ReadRequest)(views.Result,error){result:=views.Result{ViewID:id};if request.Source.ConnectionID!=config.Record.ConnectionID||request.Source.PersonID!=config.Record.PersonID||request.Source.Incarnation!=config.Record.Incarnation||request.Source.Epoch!=config.Record.Epoch||request.Source.ConnectionRevision!=config.Record.Revision||request.Query.ViewID!=id{return result,errors.New("source changed")};var value any;var err error
   switch id{case views.Calendar:q:=request.Query.Calendar;if q==nil||captured.Calendar==nil{return result,views.ErrInvalid};value,err=captured.Calendar.ReadCalendarView(ctx,time.UnixMilli(q.RangeStartUnixMS),time.UnixMilli(q.RangeEndUnixMS),q.Cursor,q.Limit);result.Calendar=&views.CalendarView{}
   case views.Communication:q:=request.Query.Mail;if q==nil||captured.Communication==nil{return result,views.ErrInvalid};value,err=captured.Communication.ReadCommunicationView(ctx,q.Query,q.Cursor,q.Limit);result.Communication=&views.CommunicationView{}
   case views.WorkContext:if request.Query.Work==nil||captured.Work==nil{return result,views.ErrInvalid};value,err=captured.Work.ReadWorkContextView(ctx);result.Work=&views.WorkContextView{}
   case views.Logistics:if request.Query.Logistics==nil||captured.Logistics==nil{return result,views.ErrInvalid};value,err=captured.Logistics.ReadLogisticsView(ctx);result.Logistics=&views.LogisticsView{}}
   if err!=nil{return views.Result{},errors.New("source read unavailable")};raw,err:=json.Marshal(value);if err!=nil{return views.Result{},views.ErrInvalid};var output any;switch id{case views.Calendar:output=result.Calendar;case views.Communication:output=result.Communication;case views.WorkContext:output=result.Work;case views.Logistics:output=result.Logistics};if trust.DecodeStrict(raw,output,1<<20,32)!=nil{return views.Result{},views.ErrInvalid};if _,_,err=views.EncodeBounded(result,request.Bounds);err!=nil{return views.Result{},err};return result,nil})
 };return runtime
}
