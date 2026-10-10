package microsoftauth

import (
	"bytes"
	"context"
	"crypto"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"math/big"
	"net/http"
	"strings"
	"time"
)

const maxIdentityResponseBytes = 64 << 10

func (runtime *Runtime) requiresProviderIdentity() bool {
	return runtime.credentialName == calendarCredentialName
}

func (runtime *Runtime) requestedScope() string {
	value := "offline_access " + runtime.scope
	if runtime.requiresProviderIdentity() {
		value += " " + openidScope + " " + profileScope
	}
	return value
}

func (runtime *Runtime) verifyIDToken(requestContext context.Context, token string, requireNonce bool, expectedNonce string) (string, error) {
	identityContext, cancel := context.WithTimeout(requestContext, 10*time.Second)
	defer cancel()
	if len(token) == 0 || len(token) > 16384 {
		return "", ErrCredentialExpired
	}
	parts := strings.Split(token, ".")
	if len(parts) != 3 || parts[0] == "" || parts[1] == "" || parts[2] == "" {
		return "", ErrCredentialExpired
	}
	headerBytes, err := decodeJWTPart(parts[0])
	if err != nil {
		return "", ErrCredentialExpired
	}
	payloadBytes, err := decodeJWTPart(parts[1])
	if err != nil {
		return "", ErrCredentialExpired
	}
	signature, err := decodeJWTPart(parts[2])
	if err != nil || len(signature) == 0 || len(signature) > 512 {
		return "", ErrCredentialExpired
	}
	header, err := decodeJWTHeader(headerBytes)
	if err != nil || header.Alg != "RS256" || header.Kid == "" || len(header.Kid) > 256 {
		return "", ErrCredentialExpired
	}
	claims, err := parseClaims(payloadBytes)
	if err != nil {
		return "", ErrCredentialExpired
	}
	if err := validateClaims(claims, runtime.config.ClientID, requireNonce, expectedNonce); err != nil {
		return "", err
	}
	issuer, err := issuerForTenant(claims.Issuer, claims.TenantID)
	if err != nil {
		return "", ErrCredentialExpired
	}
	metadata, keys, err := runtime.fetchSigningMaterial(identityContext, issuer, header.Kid)
	if err != nil {
		return "", err
	}
	if metadata.Issuer != "https://login.microsoftonline.com/common/v2.0" && metadata.Issuer != "https://login.microsoftonline.com/{tenantid}/v2.0" {
		return "", ErrCredentialExpired
	}
	key, ok := keys[header.Kid]
	if !ok {
		return "", ErrCredentialExpired
	}
	if key.Issuer != issuer && key.Issuer != "https://login.microsoftonline.com/{tenantid}/v2.0" {
		return "", ErrCredentialExpired
	}
	signed := []byte(parts[0] + "." + parts[1])
	digest := sha256.Sum256(signed)
	if err := rsa.VerifyPKCS1v15(key.PublicKey, crypto.SHA256, digest[:], signature); err != nil {
		return "", ErrCredentialExpired
	}
	return "microsoft:" + claims.TenantID + ":" + claims.Subject, nil
}

type jwtHeader struct {
	Alg string `json:"alg"`
	Kid string `json:"kid"`
	Typ string `json:"typ,omitempty"`
	X5T string `json:"x5t,omitempty"`
}

type tokenClaims struct {
	Issuer    string
	Audience  []string
	TenantID  string
	Subject   string
	Nonce     string
	ObjectID  string
	Expires   int64
	NotBefore int64
	IssuedAt  int64
}

type oidcMetadata struct {
	Issuer  string `json:"issuer"`
	JWKSURI string `json:"jwks_uri"`
}

type signingKey struct {
	Issuer    string
	PublicKey *rsa.PublicKey
}

func decodeJWTPart(value string) ([]byte, error) {
	decoded, err := base64.RawURLEncoding.DecodeString(value)
	if err != nil || base64.RawURLEncoding.EncodeToString(decoded) != value || len(decoded) > maxIdentityResponseBytes {
		return nil, errors.New("invalid jwt encoding")
	}
	return decoded, nil
}

func parseClaims(data []byte) (tokenClaims, error) {
	if err := rejectDuplicateJSON(data); err != nil {
		return tokenClaims{}, err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return tokenClaims{}, err
	}
	for _, key := range []string{"iss", "aud", "tid", "sub", "nonce", "oid", "exp", "nbf", "iat"} {
		for field := range fields {
			if field != key && strings.EqualFold(field, key) {
				return tokenClaims{}, errors.New("claim field case")
			}
		}
	}
	readString := func(name string, required bool) (string, error) {
		raw, exists := fields[name]
		if !exists {
			if required {
				return "", errors.New("missing claim")
			}
			return "", nil
		}
		var value string
		if json.Unmarshal(raw, &value) != nil || len(value) > 1024 {
			return "", errors.New("invalid claim")
		}
		return value, nil
	}
	readInt := func(name string, required bool) (int64, error) {
		raw, exists := fields[name]
		if !exists {
			if required {
				return 0, errors.New("missing claim")
			}
			return 0, nil
		}
		var value int64
		if json.Unmarshal(raw, &value) != nil || value <= 0 {
			return 0, errors.New("invalid claim")
		}
		return value, nil
	}
	issuer, err := readString("iss", true)
	if err != nil {
		return tokenClaims{}, err
	}
	tenantID, err := readString("tid", true)
	if err != nil {
		return tokenClaims{}, err
	}
	subject, err := readString("sub", true)
	if err != nil || !validIdentityPart(tenantID) || !validIdentityPart(subject) {
		return tokenClaims{}, errors.New("invalid subject")
	}
	nonce, err := readString("nonce", false)
	if err != nil {
		return tokenClaims{}, err
	}
	objectID, err := readString("oid", false)
	if err != nil {
		return tokenClaims{}, err
	}
	expires, err := readInt("exp", true)
	if err != nil {
		return tokenClaims{}, err
	}
	notBefore, err := readInt("nbf", false)
	if err != nil {
		return tokenClaims{}, err
	}
	issuedAt, err := readInt("iat", true)
	if err != nil {
		return tokenClaims{}, err
	}
	var audience []string
	rawAudience, exists := fields["aud"]
	if !exists {
		return tokenClaims{}, errors.New("missing audience")
	}
	var single string
	if json.Unmarshal(rawAudience, &single) == nil {
		audience = []string{single}
	} else if json.Unmarshal(rawAudience, &audience) != nil || len(audience) != 1 {
		return tokenClaims{}, errors.New("invalid audience")
	}
	for _, value := range audience {
		if !validIdentityPart(value) {
			return tokenClaims{}, errors.New("invalid audience")
		}
	}
	return tokenClaims{Issuer: issuer, Audience: audience, TenantID: tenantID, Subject: subject, Nonce: nonce, ObjectID: objectID, Expires: expires, NotBefore: notBefore, IssuedAt: issuedAt}, nil
}

func validateClaims(claims tokenClaims, clientID string, requireNonce bool, expectedNonce string) error {
	now := time.Now().Unix()
	if claims.Expires <= now || claims.Expires > now+24*60*60 || claims.IssuedAt > now+5*60 || claims.IssuedAt < now-24*60*60 || claims.NotBefore > now+5*60 || claims.NotBefore != 0 && claims.NotBefore > now || claims.Issuer == "" || !contains(claims.Audience, clientID) {
		return ErrCredentialExpired
	}
	if requireNonce && claims.Nonce != expectedNonce {
		return ErrCredentialExpired
	}
	return nil
}

func issuerForTenant(issuer, tenantID string) (string, error) {
	expected := "https://login.microsoftonline.com/" + tenantID + "/v2.0"
	if issuer != expected || !validGUID(tenantID) {
		return "", ErrCredentialExpired
	}
	return expected, nil
}

func (runtime *Runtime) fetchSigningMaterial(requestContext context.Context, issuer, keyID string) (oidcMetadata, map[string]signingKey, error) {
	if !runtime.allowTestEndpoints && runtime.metadataURL != defaultMetadataURL {
		return oidcMetadata{}, nil, ErrCredentialExpired
	}
	metadataBytes, err := runtime.fetchBounded(requestContext, runtime.metadataURL, "metadata")
	if err != nil {
		return oidcMetadata{}, nil, err
	}
	var metadata oidcMetadata
	if err := decodeMetadata(metadataBytes, &metadata); err != nil || metadata.Issuer == "" || metadata.JWKSURI == "" {
		return oidcMetadata{}, nil, ErrCredentialExpired
	}
	if !runtime.allowTestEndpoints && metadata.JWKSURI != "https://login.microsoftonline.com/common/discovery/v2.0/keys" {
		return oidcMetadata{}, nil, ErrCredentialExpired
	}
	keysBytes, err := runtime.fetchBounded(requestContext, metadata.JWKSURI, "jwks")
	if err != nil {
		return oidcMetadata{}, nil, err
	}
	keys, err := parseJWKS(keysBytes)
	if err != nil {
		return oidcMetadata{}, nil, err
	}
	if _, exists := keys[keyID]; !exists {
		keysBytes, err = runtime.fetchBounded(requestContext, metadata.JWKSURI, "jwks-refresh")
		if err != nil {
			return metadata, keys, err
		}
		keys, err = parseJWKS(keysBytes)
		if err != nil {
			return metadata, keys, err
		}
	}
	if _, exists := keys[keyID]; !exists {
		return metadata, keys, ErrCredentialExpired
	}
	return metadata, keys, nil
}

func (runtime *Runtime) fetchBounded(requestContext context.Context, endpoint, _ string) ([]byte, error) {
	if !runtime.allowTestEndpoints && endpoint != defaultMetadataURL && endpoint != "https://login.microsoftonline.com/common/discovery/v2.0/keys" {
		return nil, ErrCredentialExpired
	}
	request, err := http.NewRequestWithContext(requestContext, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, ErrUnavailable
	}
	response, err := runtime.client.Do(request)
	if err != nil {
		return nil, ErrUnavailable
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK || response.ContentLength > maxIdentityResponseBytes {
		return nil, ErrUnavailable
	}
	body, err := io.ReadAll(io.LimitReader(response.Body, maxIdentityResponseBytes+1))
	if err != nil || len(body) > maxIdentityResponseBytes {
		return nil, ErrUnavailable
	}
	return body, nil
}

func parseJWKS(data []byte) (map[string]signingKey, error) {
	if err := rejectDuplicateJSON(data); err != nil {
		return nil, err
	}
	var document map[string]json.RawMessage
	if err := json.Unmarshal(data, &document); err != nil {
		return nil, ErrCredentialExpired
	}
	for name := range document {
		if name != "keys" {
			return nil, ErrCredentialExpired
		}
	}
	var rawKeys []json.RawMessage
	if raw, exists := document["keys"]; !exists || json.Unmarshal(raw, &rawKeys) != nil || len(rawKeys) == 0 || len(rawKeys) > 32 {
		return nil, ErrCredentialExpired
	}
	keys := map[string]signingKey{}
	for _, raw := range rawKeys {
		if len(raw) == 0 || len(raw) > maxIdentityResponseBytes || rejectDuplicateJSON(raw) != nil {
			return nil, ErrCredentialExpired
		}
		var fields map[string]json.RawMessage
		if json.Unmarshal(raw, &fields) != nil {
			return nil, ErrCredentialExpired
		}
		allowed := map[string]bool{"kty": true, "use": true, "alg": true, "kid": true, "n": true, "e": true, "issuer": true, "x5t": true, "x5c": true, "cloud_instance_name": true}
		for name := range fields {
			if !allowed[name] {
				return nil, ErrCredentialExpired
			}
			for known := range allowed {
				if name != known && strings.EqualFold(name, known) {
					return nil, ErrCredentialExpired
				}
			}
		}
		readString := func(name string, required bool) (string, error) {
			rawValue, exists := fields[name]
			if !exists {
				if required {
					return "", ErrCredentialExpired
				}
				return "", nil
			}
			var value string
			if json.Unmarshal(rawValue, &value) != nil || len(value) > 4096 {
				return "", ErrCredentialExpired
			}
			return value, nil
		}
		keyType, err := readString("kty", true)
		if err != nil {
			return nil, err
		}
		use, err := readString("use", true)
		if err != nil {
			return nil, err
		}
		algorithm, err := readString("alg", false)
		if err != nil || algorithm != "" && algorithm != "RS256" {
			return nil, ErrCredentialExpired
		}
		kid, err := readString("kid", true)
		if err != nil {
			return nil, err
		}
		modulusText, err := readString("n", true)
		if err != nil {
			return nil, err
		}
		exponentText, err := readString("e", true)
		if err != nil {
			return nil, err
		}
		issuer, err := readString("issuer", true)
		if err != nil {
			return nil, err
		}
		cloudInstance, err := readString("cloud_instance_name", false)
		if err != nil || strings.ContainsAny(cloudInstance, "/\\\r\n") {
			return nil, ErrCredentialExpired
		}
		if keyType != "RSA" || kid == "" || modulusText == "" || exponentText == "" || use != "sig" || !validSigningKeyIssuer(issuer) {
			return nil, ErrCredentialExpired
		}
		if _, exists := keys[kid]; exists {
			return nil, ErrCredentialExpired
		}
		modulus, err := decodeJWTPart(modulusText)
		if err != nil {
			return nil, ErrCredentialExpired
		}
		exponentBytes, err := decodeJWTPart(exponentText)
		if err != nil || len(exponentBytes) > 4 {
			return nil, ErrCredentialExpired
		}
		exponent := 0
		for _, value := range exponentBytes {
			exponent = exponent<<8 | int(value)
		}
		if exponent < 3 || exponent%2 == 0 || len(modulus) < 256 || len(modulus) > 512 {
			return nil, ErrCredentialExpired
		}
		keys[kid] = signingKey{Issuer: issuer, PublicKey: &rsa.PublicKey{N: new(big.Int).SetBytes(modulus), E: exponent}}
	}
	return keys, nil
}

func validSigningKeyIssuer(value string) bool {
	const prefix = "https://login.microsoftonline.com/"
	const suffix = "/v2.0"
	if !strings.HasPrefix(value, prefix) || !strings.HasSuffix(value, suffix) {
		return false
	}
	tenant := strings.TrimSuffix(strings.TrimPrefix(value, prefix), suffix)
	return tenant == "{tenantid}" || validGUID(tenant)
}

func decodeStrictJSON(data []byte, target any) error {
	if len(data) == 0 || len(data) > maxIdentityResponseBytes || rejectDuplicateJSON(data) != nil {
		return ErrCredentialExpired
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return err
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		return errors.New("json trailing")
	}
	return nil
}

func decodeMetadata(data []byte, target *oidcMetadata) error {
	if len(data) == 0 || len(data) > maxIdentityResponseBytes || rejectDuplicateJSON(data) != nil {
		return ErrCredentialExpired
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	for _, key := range []string{"issuer", "jwks_uri"} {
		for name := range fields {
			if name != key && strings.EqualFold(name, key) {
				return ErrCredentialExpired
			}
		}
	}
	if raw, exists := fields["issuer"]; exists {
		if err := json.Unmarshal(raw, &target.Issuer); err != nil {
			return err
		}
	}
	if raw, exists := fields["jwks_uri"]; exists {
		if err := json.Unmarshal(raw, &target.JWKSURI); err != nil {
			return err
		}
	}
	return nil
}

func decodeJWTHeader(data []byte) (jwtHeader, error) {
	if len(data) == 0 || rejectDuplicateJSON(data) != nil {
		return jwtHeader{}, ErrCredentialExpired
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return jwtHeader{}, err
	}
	allowed := map[string]bool{"alg": true, "kid": true, "typ": true, "x5t": true}
	for key := range fields {
		if !allowed[key] {
			return jwtHeader{}, ErrCredentialExpired
		}
		for known := range allowed {
			if key != known && strings.EqualFold(key, known) {
				return jwtHeader{}, ErrCredentialExpired
			}
		}
	}
	var header jwtHeader
	decode := func(key string, target any) error {
		raw, exists := fields[key]
		if !exists {
			return nil
		}
		return json.Unmarshal(raw, target)
	}
	for key, target := range map[string]any{"alg": &header.Alg, "kid": &header.Kid, "typ": &header.Typ, "x5t": &header.X5T} {
		if err := decode(key, target); err != nil {
			return jwtHeader{}, ErrCredentialExpired
		}
	}
	return header, nil
}

func decodeDocument(data []byte, target any) error {
	if len(data) == 0 || len(data) > maxIdentityResponseBytes || rejectDuplicateJSON(data) != nil {
		return ErrCredentialExpired
	}
	if err := json.Unmarshal(data, target); err != nil {
		return err
	}
	return nil
}

func rejectDuplicateJSON(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	if err := walkJSON(decoder, 0); err != nil {
		return err
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return errors.New("json trailing")
	}
	return nil
}

func walkJSON(decoder *json.Decoder, depth int) error {
	if depth > 16 {
		return errors.New("json depth")
	}
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	if delimiter, ok := token.(json.Delim); ok {
		switch delimiter {
		case '{':
			fields := map[string]bool{}
			for decoder.More() {
				key, err := decoder.Token()
				if err != nil {
					return err
				}
				name, ok := key.(string)
				if !ok || fields[name] {
					return errors.New("json duplicate")
				}
				fields[name] = true
				if err := walkJSON(decoder, depth+1); err != nil {
					return err
				}
			}
			_, err := decoder.Token()
			return err
		case '[':
			for decoder.More() {
				if err := walkJSON(decoder, depth+1); err != nil {
					return err
				}
			}
			_, err := decoder.Token()
			return err
		}
	}
	return nil
}

func validIdentityPart(value string) bool {
	return value != "" && len(value) <= 255 && strings.TrimSpace(value) == value && !strings.ContainsAny(value, "\r\n\x00")
}
func validGUID(value string) bool {
	if len(value) != 36 {
		return false
	}
	for index, character := range value {
		if index == 8 || index == 13 || index == 18 || index == 23 {
			if character != '-' {
				return false
			}
			continue
		}
		if !((character >= '0' && character <= '9') || (character >= 'a' && character <= 'f') || (character >= 'A' && character <= 'F')) {
			return false
		}
	}
	return true
}
func contains(values []string, expected string) bool {
	for _, value := range values {
		if value == expected {
			return true
		}
	}
	return false
}
