package views

import (
    "crypto/sha256"
    "encoding/hex"
    "encoding/json"
    "strings"
    "time"
    "unicode"
    "unicode/utf8"
)

const CalendarMirror ID = "calendar.mirror"

type CalendarMirrorQuery struct {
    CalendarID string `json:"calendar_id"`
    RangeStartUnixMS int64 `json:"range_start_unix_ms"`
    RangeEndUnixMS int64 `json:"range_end_unix_ms"`
    Cursor string `json:"cursor"`
    Limit uint32 `json:"limit"`
}
type CalendarExternalRevision struct {
    Kind string `json:"kind"`
    Value string `json:"value,omitempty"`
    SHA256 string `json:"sha256,omitempty"`
}
type CalendarSchedule struct {
    Kind string `json:"kind"`
    StartsAt string `json:"starts_at,omitempty"`
    EndsAt string `json:"ends_at,omitempty"`
    Timezone string `json:"timezone,omitempty"`
    StartDate string `json:"start_date,omitempty"`
    EndDateExclusive string `json:"end_date_exclusive,omitempty"`
}
type CalendarRecord struct {
    CanModify bool `json:"can_modify"`
    CalendarID string `json:"calendar_id"`
    ExternalID string `json:"external_id"`
    ExternalRevision CalendarExternalRevision `json:"external_revision"`
    Title string `json:"title"`
    Schedule CalendarSchedule `json:"schedule"`
}
// CalendarMirrorResult is the normalized provider page before authority adds its
// signed operation/source envelope and replaces the provider cursor with a token.
type CalendarMirrorResult struct {
    CalendarID string
    RangeStartUnixMS,RangeEndUnixMS int64
    ObservedAtUnixMS,ExpiresAtUnixMS int64
    Records []CalendarRecord
    NextCursor string
}

func ParseMirrorQuery(raw []byte)(CalendarMirrorQuery,error){
    var q CalendarMirrorQuery
    if !strictFlat(raw,&q,[]string{"calendar_id","range_start_unix_ms","range_end_unix_ms","cursor","limit"}) || !validMirrorText(q.CalendarID,256) || q.RangeStartUnixMS<0 || q.RangeEndUnixMS<=q.RangeStartUnixMS || q.RangeEndUnixMS-q.RangeStartUnixMS>int64(48*time.Hour/time.Millisecond) || len(q.Cursor)>4096 || strings.ContainsFunc(q.Cursor,unicode.IsControl) || q.Limit==0 || q.Limit>128 {return q,ErrInvalid}
    return q,nil
}
func ValidateMirrorResult(result CalendarMirrorResult,request ReadRequest)error{
    q:=request.Query.Mirror
    if request.Query.ViewID!=CalendarMirror || q==nil || result.CalendarID!=q.CalendarID || result.RangeStartUnixMS!=q.RangeStartUnixMS || result.RangeEndUnixMS!=q.RangeEndUnixMS || result.ObservedAtUnixMS<=0 || result.ExpiresAtUnixMS<=result.ObservedAtUnixMS || len(result.NextCursor)>4096 || strings.ContainsFunc(result.NextCursor,unicode.IsControl) || result.Records==nil || len(result.Records)>int(q.Limit) || uint32(len(result.Records))>request.Bounds.MaxItems {return ErrInvalid}
    selected:=false
    for _,id:=range request.Source.Resources{selected=selected || id==q.CalendarID}
    if !selected{return ErrInvalid}
    seen:=map[string]bool{}
    for _,r:=range result.Records{
        if r.CanModify || r.CalendarID!=q.CalendarID || !validMirrorText(r.ExternalID,512) || seen[r.ExternalID] || len(r.Title)>4096 || !utf8.ValidString(r.Title) || !validMirrorRevision(r.ExternalRevision) || !validMirrorSchedule(r.Schedule){return ErrInvalid}
        seen[r.ExternalID]=true
    }
    raw,err:=json.Marshal(result.Records)
    if err!=nil || request.Bounds.MaxBytes==0 || request.Bounds.MaxBytes>1<<20 || len(raw)>int(request.Bounds.MaxBytes){return ErrInvalid}
    return nil
}
func validMirrorRevision(r CalendarExternalRevision)bool{
    switch r.Kind{
    case "provider_opaque":return validMirrorText(r.Value,512) && r.SHA256==""
    case "observation_fingerprint":
        if r.Value!="" || len(r.SHA256)!=64 || strings.ToLower(r.SHA256)!=r.SHA256{return false}
        b,err:=hex.DecodeString(r.SHA256);if err!=nil{return false};nonzero:=false;for _,v:=range b{nonzero=nonzero || v!=0};return nonzero
    default:return false
    }
}
func validMirrorSchedule(s CalendarSchedule)bool{
    switch s.Kind{
    case "timed":
        if s.StartDate!="" || s.EndDateExclusive!="" || !validMirrorText(s.Timezone,128) || !strings.HasSuffix(s.StartsAt,"Z") || !strings.HasSuffix(s.EndsAt,"Z"){return false}
        start,e1:=time.Parse(time.RFC3339Nano,s.StartsAt);end,e2:=time.Parse(time.RFC3339Nano,s.EndsAt)
        return e1==nil && e2==nil && start.Before(end)
    case "all_day":
        if s.StartsAt!="" || s.EndsAt!="" || s.Timezone!=""{return false}
        start,e1:=time.Parse("2006-01-02",s.StartDate);end,e2:=time.Parse("2006-01-02",s.EndDateExclusive)
        return e1==nil && e2==nil && start.Before(end)
    default:return false
    }
}
func validMirrorText(s string,max int)bool{return strings.TrimSpace(s)!="" && len(s)<=max && utf8.ValidString(s) && !strings.ContainsFunc(s,unicode.IsControl)}

// ObservationRevision never claims a provider conditional-write token.
func ObservationRevision(record CalendarRecord) CalendarExternalRevision {
    raw,_:=json.Marshal(struct{CalendarID,ExternalID,Title string;Schedule CalendarSchedule}{record.CalendarID,record.ExternalID,record.Title,record.Schedule})
    digest:=sha256.Sum256(raw)
    return CalendarExternalRevision{Kind:"observation_fingerprint",SHA256:hex.EncodeToString(digest[:])}
}
