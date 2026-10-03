package gmail

import (
 "floe/server/internal/views"
 "context"
 
)
// Reader is the contextual boundary for the existing normalized Gmail index.
type Reader struct { Service *Service }
func(r Reader)ConnectionSnapshot(ctx context.Context)(any,error){if err:=ctx.Err();err!=nil{return nil,err};return r.Service.ConnectionSnapshot()}
func(r Reader)ReadCommunicationView(ctx context.Context,q string,cursor,limit int)(any,error){if err:=ctx.Err();err!=nil{return nil,err};return r.Service.ReadCommunicationView(q,cursor,limit)}
func(r Reader)ReadLogisticsView(ctx context.Context)(views.LogisticsView,error){return r.Service.ReadLogisticsView(ctx)}
