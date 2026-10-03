package views

import (
 "bytes"
 "context"
 "crypto/sha256"
 "encoding/json"
 "errors"
 "io"
 "strings"
 "unicode/utf8"
)
type ID string
const(Calendar ID="calendar.timeline";Communication ID="mail.communication";WorkContext ID="work.context";Logistics ID="life.logistics")
type SourceTarget struct{ViewID ID;ConnectorID,ConnectionID string;ConnectionRevision uint64;ResourceID string}
type SourceReference struct{ConnectorID,ConnectionID,ExecutionOwner,Incarnation string;Epoch uint64}
type SourceSnapshot struct{SourceReference;ConnectionRevision uint64;PersonID,DeviceID,ProviderIdentity string;IdentityGeneration uint64;Resources []string;Active bool;Descriptor ViewDescriptor}
type Bounds struct{MaxItems,MaxBytes uint32}
type CalendarQuery struct{RangeStartUnixMS int64 `json:"range_start_unix_ms"`;RangeEndUnixMS int64 `json:"range_end_unix_ms"`;Cursor string `json:"cursor"`;Limit int `json:"limit"`}
type MailQuery struct{Query string `json:"query"`;Cursor int `json:"cursor"`;Limit int `json:"limit"`}
type WorkQuery struct{SchemaVersion int `json:"schema_version"`}
type LogisticsQuery struct{SchemaVersion int `json:"schema_version"`}
type Query struct{ViewID ID;Calendar *CalendarQuery;Mail *MailQuery;Work *WorkQuery;Logistics *LogisticsQuery}
type ParsedQuery struct{Query Query;Canonical []byte;Digest [32]byte}
type ReadRequest struct{Source SourceSnapshot;Query Query;Bounds Bounds}
type Reader interface{Read(context.Context,ReadRequest)(Result,error)}
type ReaderFunc func(context.Context,ReadRequest)(Result,error)
func(f ReaderFunc)Read(ctx context.Context,r ReadRequest)(Result,error){return f(ctx,r)}
type Result struct{ViewID ID;Calendar *CalendarView;Communication *CommunicationView;Work *WorkContextView;Logistics *LogisticsView}
var ErrInvalid=errors.New("invalid normalized view")
func ParseQuery(id ID,raw []byte)(ParsedQuery,error){q:=Query{ViewID:id};var value any;var fields []string;switch id{case Calendar:q.Calendar=&CalendarQuery{};value=q.Calendar;fields=[]string{"range_start_unix_ms","range_end_unix_ms","cursor","limit"};case Communication:q.Mail=&MailQuery{};value=q.Mail;fields=[]string{"query","cursor","limit"};case WorkContext:q.Work=&WorkQuery{};value=q.Work;fields=[]string{"schema_version"};case Logistics:q.Logistics=&LogisticsQuery{};value=q.Logistics;fields=[]string{"schema_version"};default:return ParsedQuery{},ErrInvalid};if !strictFlat(raw,value,fields){return ParsedQuery{},ErrInvalid};switch id{case Calendar:v:=q.Calendar;if v.RangeStartUnixMS<0||v.RangeEndUnixMS<=v.RangeStartUnixMS||v.RangeEndUnixMS-v.RangeStartUnixMS>2764800000||len(v.Cursor)>2048||strings.ContainsAny(v.Cursor,"\r\n\x00")||v.Limit<1||v.Limit>128{return ParsedQuery{},ErrInvalid};case Communication:v:=q.Mail;if len(v.Query)>512||strings.ContainsAny(v.Query,"\r\n\x00")||v.Cursor<0||v.Cursor>10000||v.Limit<1||v.Limit>100{return ParsedQuery{},ErrInvalid};case WorkContext:if q.Work.SchemaVersion!=1{return ParsedQuery{},ErrInvalid};case Logistics:if q.Logistics.SchemaVersion!=1{return ParsedQuery{},ErrInvalid}};encoded,err:=json.Marshal(value);if err!=nil{return ParsedQuery{},ErrInvalid};return ParsedQuery{q,encoded,sha256.Sum256(encoded)},nil}
func strictFlat(raw []byte,out any,fields []string)bool{if len(raw)==0||len(raw)>8192||!utf8.Valid(raw){return false};d:=json.NewDecoder(bytes.NewReader(raw));token,err:=d.Token();if err!=nil||token!=json.Delim('{'){return false};allowed:=map[string]bool{};for _,k:=range fields{allowed[k]=true};seen:=map[string]bool{};for d.More(){token,err=d.Token();key,ok:=token.(string);if err!=nil||!ok||!allowed[key]||seen[key]{return false};seen[key]=true;var value json.RawMessage;if d.Decode(&value)!=nil||bytes.Equal(bytes.TrimSpace(value),[]byte("null")){return false}};end,err:=d.Token();if err!=nil||end!=json.Delim('}')||d.Decode(new(any))!=io.EOF||len(seen)!=len(fields){return false};decoder:=json.NewDecoder(bytes.NewReader(raw));decoder.DisallowUnknownFields();return decoder.Decode(out)==nil}
func EncodeBounded(result Result,b Bounds)([]byte,uint32,error){count:=0;for _,present:=range []bool{result.Calendar!=nil,result.Communication!=nil,result.Work!=nil,result.Logistics!=nil}{if present{count++}};if count!=1||b.MaxItems==0||b.MaxItems>128||b.MaxBytes==0||b.MaxBytes>1<<20{return nil,0,ErrInvalid};var payload any;var items uint32;var schema int;var id,source string;var observed,expires int64;handles:=[]string{}
 switch result.ViewID{case Calendar:v:=result.Calendar;if v==nil{return nil,0,ErrInvalid};payload=v;schema,id,source,observed,expires=v.SchemaVersion,v.ViewID,v.SourceHandle,v.ObservedAtUnixMS,v.ExpiresAtUnixMS;items=uint32(len(v.Items));if v.RangeStartUnixMS<0||v.RangeEndUnixMS<=v.RangeStartUnixMS||v.RangeEndUnixMS-v.RangeStartUnixMS>2764800000||v.NextCursor!=nil&&(len(*v.NextCursor)>2048||strings.ContainsAny(*v.NextCursor,"\r\n\x00")){return nil,0,ErrInvalid};for _,i:=range v.Items{if i.StartsAtUnixMS<0||i.EndsAtUnixMS<i.StartsAtUnixMS||len(i.UntrustedTitle)>4096||!utf8.ValidString(i.UntrustedTitle){return nil,0,ErrInvalid};handles=append(handles,i.EvidenceHandle)}
 case Communication:v:=result.Communication;if v==nil{return nil,0,ErrInvalid};payload=v;schema,id,source,observed,expires=v.SchemaVersion,v.ViewID,v.SourceHandle,v.ObservedAtUnixMS,v.ExpiresAtUnixMS;items=uint32(len(v.Items));if v.NextCursor!=nil&&(*v.NextCursor<0||*v.NextCursor>10000){return nil,0,ErrInvalid};for _,i:=range v.Items{if i.ReceivedUnixMS<0||i.ThreadHandle==""||len(i.ThreadHandle)>256||len(i.Subject)>4096||len(i.Snippet)>8192||len(i.From)>1024||len(i.To)>4096||len(i.Labels)>64{return nil,0,ErrInvalid};handles=append(handles,i.EvidenceHandle)}
 case WorkContext:v:=result.Work;if v==nil{return nil,0,ErrInvalid};payload=v;schema,id,source,observed,expires=v.SchemaVersion,v.ViewID,v.SourceHandle,v.ObservedAtUnixMS,v.ExpiresAtUnixMS;items=uint32(len(v.Items));if v.ScopeHandle==""||len(v.ScopeHandle)>256{return nil,0,ErrInvalid};for _,i:=range v.Items{if i.Kind==""||len(i.Kind)>128||len(i.Title)>4096||i.ObservedAtUnixMS<0||i.ObservedAtUnixMS>observed{return nil,0,ErrInvalid};for _,p:=range []*string{i.Excerpt,i.Status,i.Blocker,i.NextAction}{if p!=nil&&len(*p)>8192{return nil,0,ErrInvalid}};handles=append(handles,i.EvidenceHandle)}
 case Logistics:v:=result.Logistics;if v==nil{return nil,0,ErrInvalid};payload=v;schema,id,source,observed,expires=v.SchemaVersion,v.ViewID,v.SourceHandle,v.ObservedAtUnixMS,v.ExpiresAtUnixMS;items=uint32(len(v.Items));for _,i:=range v.Items{if i.Kind==""||len(i.Kind)>128||len(i.Summary)>8192||len(i.Status)>128||i.OccursAtUnixMS!=nil&&*i.OccursAtUnixMS<0{return nil,0,ErrInvalid};handles=append(handles,i.EvidenceHandle)}
 default:return nil,0,ErrInvalid}
 if schema!=1||id!=string(result.ViewID)||source==""||len(source)>256||strings.ContainsAny(source,"\r\n\x00")||observed<=0||expires<=observed||items>b.MaxItems{return nil,0,ErrInvalid};seen:=map[string]bool{};for _,h:=range handles{if h==""||len(h)>256||seen[h]||strings.ContainsAny(h,"\r\n\x00"){return nil,0,ErrInvalid};seen[h]=true};data,err:=json.Marshal(payload);if err!=nil||len(data)>int(b.MaxBytes)||!utf8.Valid(data){return nil,0,ErrInvalid};return data,items,nil}
