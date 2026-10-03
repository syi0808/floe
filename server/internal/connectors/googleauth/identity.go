package googleauth

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

const maxIdentityResponseBytes = 8192

func (runtime *Runtime) requiresProviderIdentity() bool {
	return hasScope(strings.Join(runtime.scopes, " "), openidScope)
}

func (runtime *Runtime) identityForToken(ctx context.Context, accessToken string) (string, error) {
	if !runtime.requiresProviderIdentity() || !validCredential(accessToken, 16384) {
		return "", ErrCredentialExpired
	}
	if !runtime.allowTestEndpoints && runtime.userinfoURL != defaultUserInfoURL {
		return "", ErrUnavailable
	}
	identityContext, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	request, err := http.NewRequestWithContext(identityContext, http.MethodGet, runtime.userinfoURL, nil)
	if err != nil {
		return "", ErrUnavailable
	}
	request.Header.Set("Authorization", "Bearer "+accessToken)
	request.Header.Set("Accept", "application/json")
	response, err := runtime.client.Do(request)
	if err != nil {
		if ctx.Err() != nil {
			return "", ctx.Err()
		}
		return "", ErrUnavailable
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		if response.StatusCode == http.StatusUnauthorized {
			return "", ErrCredentialExpired
		}
		return "", ErrUnavailable
	}
	if response.ContentLength > maxIdentityResponseBytes {
		return "", ErrUnavailable
	}
	body, err := io.ReadAll(io.LimitReader(response.Body, maxIdentityResponseBytes+1))
	if err != nil || len(body) > maxIdentityResponseBytes {
		return "", ErrUnavailable
	}
	value, err := decodeGoogleUserInfo(body)
	if err != nil || !validSubject(value.Sub) {
		return "", ErrCredentialExpired
	}
	return "google:" + value.Sub, nil
}

type googleUserInfo struct {
	Sub           string `json:"sub"`
	Name          string `json:"name"`
	GivenName     string `json:"given_name"`
	FamilyName    string `json:"family_name"`
	Picture       string `json:"picture"`
	Email         string `json:"email"`
	EmailVerified bool   `json:"email_verified"`
	HostedDomain  string `json:"hd"`
	Locale        string `json:"locale"`
}

func decodeStrictIdentityJSON(data []byte, target any) error {
	if len(data) == 0 || len(data) > maxIdentityResponseBytes {
		return errors.New("identity response size")
	}
	if err := rejectDuplicateJSON(data); err != nil {
		return err
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return err
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return fmt.Errorf("identity response trailing data")
	}
	return nil
}

func decodeGoogleUserInfo(data []byte) (googleUserInfo, error) {
	if len(data) == 0 || len(data) > maxIdentityResponseBytes || rejectDuplicateJSON(data) != nil {
		return googleUserInfo{}, errors.New("identity response")
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return googleUserInfo{}, err
	}
	allowed := map[string]bool{"sub": true, "name": true, "given_name": true, "family_name": true, "picture": true, "email": true, "email_verified": true, "hd": true, "locale": true}
	for key, raw := range fields {
		if !allowed[key] {
			return googleUserInfo{}, errors.New("identity response field")
		}
		if len(raw) > maxIdentityResponseBytes {
			return googleUserInfo{}, errors.New("identity response field size")
		}
	}
	var value googleUserInfo
	decode := func(key string, target any) error {
		raw, exists := fields[key]
		if !exists {
			return nil
		}
		return json.Unmarshal(raw, target)
	}
	for key, target := range map[string]any{"sub": &value.Sub, "name": &value.Name, "given_name": &value.GivenName, "family_name": &value.FamilyName, "picture": &value.Picture, "email": &value.Email, "email_verified": &value.EmailVerified, "hd": &value.HostedDomain, "locale": &value.Locale} {
		if err := decode(key, target); err != nil {
			return googleUserInfo{}, errors.New("identity response field type")
		}
	}
	return value, nil
}

func rejectDuplicateJSON(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	var value any
	if err := decodeJSONValue(decoder, &value, 0); err != nil {
		return err
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return errors.New("identity response trailing data")
	}
	return nil
}

func decodeJSONValue(decoder *json.Decoder, target *any, depth int) error {
	if depth > 16 {
		return errors.New("identity response nesting")
	}
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	switch value := token.(type) {
	case json.Delim:
		switch value {
		case '{':
			fields := map[string]struct{}{}
			object := map[string]any{}
			for decoder.More() {
				keyToken, err := decoder.Token()
				if err != nil {
					return err
				}
				key, ok := keyToken.(string)
				if !ok {
					return errors.New("identity response object key")
				}
				if _, exists := fields[key]; exists {
					return errors.New("identity response duplicate field")
				}
				fields[key] = struct{}{}
				var child any
				if err := decodeJSONValue(decoder, &child, depth+1); err != nil {
					return err
				}
				object[key] = child
			}
			if _, err := decoder.Token(); err != nil {
				return err
			}
			*target = object
		case '[':
			items := []any{}
			for decoder.More() {
				var child any
				if err := decodeJSONValue(decoder, &child, depth+1); err != nil {
					return err
				}
				items = append(items, child)
			}
			if _, err := decoder.Token(); err != nil {
				return err
			}
			*target = items
		default:
			return errors.New("identity response delimiter")
		}
	default:
		*target = value
	}
	return nil
}

func validSubject(value string) bool {
	if value == "" || len(value) > 255 || strings.TrimSpace(value) != value {
		return false
	}
	for _, character := range value {
		if character < 0x21 || character > 0x7e {
			return false
		}
	}
	return true
}
