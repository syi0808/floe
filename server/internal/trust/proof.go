package trust

import (
 "bytes"
 "crypto/ed25519"
 "crypto/rand"
 "crypto/sha256"
 "encoding/base64"
 "encoding/hex"
 "encoding/json"
 "errors"
 "io"
 "strconv"
 "reflect"
 "strings"
 "unicode/utf8"
)
const SignatureDomain = "floe.remote.authorization.v1\x00"
const MaxJSONInteger uint64 = 9007199254740991
var ErrInvalid = errors.New("invalid proof")
type Proof struct { ChallengeID string `json:"challenge_id"`; KeyID string `json:"key_id"`; Signature string `json:"signature"` }
func NewID() string { b:=make([]byte,16); if _,err:=rand.Read(b);err!=nil { panic("secure randomness unavailable") }; b[6]=b[6]&15|64;b[8]=b[8]&63|128;s:=hex.EncodeToString(b); return s[:8]+"-"+s[8:12]+"-"+s[12:16]+"-"+s[16:20]+"-"+s[20:] }
func Token() string { return rand.Text()+rand.Text() }
func Digest(v string) string { sum:=sha256.Sum256([]byte(v));return hex.EncodeToString(sum[:]) }
func ValidID(s string) bool { if len(s)!=36 || s[8]!='-' || s[13]!='-' || s[18]!='-' || s[23]!='-' || strings.ToLower(s)!=s { return false }; raw:=strings.ReplaceAll(s,"-","");b,err:=hex.DecodeString(raw);return err==nil&&len(b)==16&&!bytes.Equal(b,make([]byte,16)) }
func ValidDevice(s string) bool { if len(s)==0||len(s)>128{return false};for _,r:=range s { if !(r>='A'&&r<='Z'||r>='a'&&r<='z'||r>='0'&&r<='9'||strings.ContainsRune("._:-",r)){return false} };return true }
func DecodeBase64(s string, size int) ([]byte,error) { b,err:=base64.RawURLEncoding.DecodeString(s); if err!=nil||len(b)!=size||base64.RawURLEncoding.EncodeToString(b)!=s{return nil,ErrInvalid};return b,nil }
func VerifyProof(proof Proof, challengeID, keyID string, exact []byte, key ed25519.PublicKey) error {
 if proof.ChallengeID!=challengeID||proof.KeyID!=keyID||!ValidID(challengeID)||!ValidID(keyID){return ErrInvalid};sig,err:=DecodeBase64(proof.Signature,ed25519.SignatureSize);if err!=nil{return err};if !ed25519.Verify(key,append([]byte(SignatureDomain),exact...),sig){return ErrInvalid};return nil
}
// StrictJSON rejects duplicate keys, invalid UTF-8, trailing values and excess depth.
// Number validation is intentionally token based: no float conversion can round authority.
func StrictJSON(data []byte, limit, depth int) error {
 if len(data)==0||len(data)>limit||!utf8.Valid(data){return ErrInvalid};d:=json.NewDecoder(bytes.NewReader(data));d.UseNumber();if err:=walkJSON(d,0,depth);err!=nil{return err};if d.Decode(new(any))!=io.EOF{return ErrInvalid};return nil
}
func walkJSON(d *json.Decoder, depth,max int) error {
 if depth>max{return ErrInvalid};t,err:=d.Token();if err!=nil{return ErrInvalid};switch v:=t.(type) {
 case json.Number:
  // JSON schemas and argument objects may contain finite fractional numbers.
  f,err:=strconv.ParseFloat(string(v),64);if err!=nil||f>float64(MaxJSONInteger)||f< -float64(MaxJSONInteger){return ErrInvalid}
 case json.Delim:
  switch v { case '{': seen:=map[string]bool{};for d.More(){k,err:=d.Token();if err!=nil{return ErrInvalid};s,ok:=k.(string);if !ok||seen[s]{return ErrInvalid};seen[s]=true;if err:=walkJSON(d,depth+1,max);err!=nil{return err}};end,err:=d.Token();if err!=nil||end!=json.Delim('}'){return ErrInvalid}
  case '[':for d.More(){if err:=walkJSON(d,depth+1,max);err!=nil{return err}};end,err:=d.Token();if err!=nil||end!=json.Delim(']'){return ErrInvalid}
  default:return ErrInvalid }
 };return nil
}
func DecodeStrict(data []byte, out any, limit, depth int) error { if err:=StrictJSON(data,limit,depth);err!=nil{return err};if !exactShape(data,reflect.TypeOf(out)){return ErrInvalid};d:=json.NewDecoder(bytes.NewReader(data));d.DisallowUnknownFields();if d.Decode(out)!=nil||d.Decode(new(any))!=io.EOF{return ErrInvalid};return nil }

func ParseProofJSON(data []byte)(Proof,error){var p Proof;if DecodeStrict(data,&p,4096,16)!=nil||!ValidID(p.ChallengeID)||!ValidID(p.KeyID){return Proof{},ErrInvalid};if _,err:=DecodeBase64(p.Signature,ed25519.SignatureSize);err!=nil{return Proof{},err};return p,nil}

func exactShape(raw []byte,t reflect.Type)bool{
 if t==nil{return false};for t.Kind()==reflect.Pointer {if bytes.Equal(bytes.TrimSpace(raw),[]byte("null")){return true};t=t.Elem()};if t==reflect.TypeOf(json.RawMessage{}){return true}
 switch t.Kind(){case reflect.Struct:
  var fields map[string]json.RawMessage;if json.Unmarshal(raw,&fields)!=nil||fields==nil{return false};allowed:=map[string]reflect.Type{};for i:=0;i<t.NumField();i++{f:=t.Field(i);if f.PkgPath!=""{continue};tag:=strings.Split(f.Tag.Get("json"),",")[0];if tag=="-"{continue};if tag==""{tag=f.Name};allowed[tag]=f.Type};for name,value:=range fields{child,ok:=allowed[name];if !ok||!exactShape(value,child){return false}};return true
 case reflect.Map:var fields map[string]json.RawMessage;if json.Unmarshal(raw,&fields)!=nil||fields==nil{return false};for _,v:=range fields{if !exactShape(v,t.Elem()){return false}};return true
 case reflect.Slice:if t.Elem().Kind()==reflect.Uint8{return true};var elements []json.RawMessage;if json.Unmarshal(raw,&elements)!=nil||elements==nil{return false};for _,v:=range elements{if !exactShape(v,t.Elem()){return false}};return true
 case reflect.Interface:return true
 default:return !bytes.Equal(bytes.TrimSpace(raw),[]byte("null")) }
}
