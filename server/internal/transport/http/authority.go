package httptransport
import (
 "encoding/json"
 "io"
 stdhttp "net/http"
 "floe/server/internal/authority"
 "floe/server/internal/trust"
)
type AuthorityHandler struct {Trust *trust.Service}
func(h AuthorityHandler)ServeAdmin(w stdhttp.ResponseWriter,r *stdhttp.Request)bool {
 switch r.URL.Path {
 case "/manage/api/authority/producer":if r.Method!=stdhttp.MethodGet{failure(w,405,"method_not_allowed");return true};m,err:=h.Trust.ProducerMetadata();if err!=nil{failure(w,503,"producer_unavailable")}else{reply(w,200,m)}
 case "/manage/api/authority/enrollments":if r.Method!=stdhttp.MethodGet{failure(w,405,"method_not_allowed");return true};issuers,err:=h.Trust.Issuers();if err!=nil{failure(w,503,"trust_unavailable");return true};out:=[]any{};for _,i:=range issuers{out=append(out,map[string]any{"enrollment_id":i.EnrollmentID,"key_id":i.KeyID,"client_id":i.Principal.ClientID(),"person_id":i.Principal.PersonID(),"device_id":i.Principal.DeviceID(),"fingerprint":trust.Digest(string(i.PublicKey)),"active":true})};reply(w,200,map[string]any{"enrollments":out})
 case "/manage/api/authority/revoke":if r.Method!=stdhttp.MethodPost{failure(w,405,"method_not_allowed");return true};var in struct{KeyID string `json:"key_id"`};if !strictDecode(w,r,&in){failure(w,400,"validation");return true};if h.Trust.RevokeIssuer(r.Context(),in.KeyID)!=nil{failure(w,409,"revoke_failed")}else{reply(w,200,map[string]string{"status":"revoked"})}
 default:failure(w,404,"not_found")
 };return true
}
func reply(writer stdhttp.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func failure(writer stdhttp.ResponseWriter, status int, code string) {
	reply(writer, status, map[string]any{"error": map[string]string{"code": code}})
}

func strictDecode(w stdhttp.ResponseWriter,r *stdhttp.Request,out any)bool{data,err:=io.ReadAll(stdhttp.MaxBytesReader(w,r.Body,authority.MaxProofBytes));return err==nil&&trust.DecodeStrict(data,out,authority.MaxProofBytes,authority.MaxJSONDepth)==nil}
func StrictJSON(data []byte)bool{return trust.StrictJSON(data,65536,32)==nil}
