package inference

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"floe/server/internal/trust"
	"math"
	"net/mail"
	"net/url"
	"regexp"
	"sort"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

var aliasPattern = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)

func ValidAlias(s string) bool { return aliasPattern.MatchString(s) }
func ValidHex(s string, n int) bool {
	b, e := hex.DecodeString(s)
	return e == nil && len(b) == n && hex.EncodeToString(b) == s
}
func validClasses(v []string) bool {
	if len(v) < 1 || len(v) > 3 || !sort.StringsAreSorted(v) {
		return false
	}
	for i, s := range v {
		if s != "synthetic" && s != "personal" && s != "highly_sensitive" || i > 0 && s == v[i-1] {
			return false
		}
	}
	return true
}
func validCallID(s string) bool {
	if len(s) < 1 || len(s) > 128 || !utf8.ValidString(s) {
		return false
	}
	for _, r := range s {
		if unicode.IsControl(r) {
			return false
		}
	}
	return true
}
func validObject(raw []byte) bool {
	if trust.StrictJSON(raw, 32768, 32) != nil {
		return false
	}
	trim := bytes.TrimSpace(raw)
	return len(trim) > 1 && trim[0] == '{'
}
func ValidSchema(raw []byte) bool {
	if !validObject(raw) {
		return false
	}
	var schema map[string]any
	if json.Unmarshal(raw, &schema) != nil || schema["type"] != "object" {
		return false
	}
	return validSchemaValue(schema, 0)
}
func validSchemaValue(s map[string]any, depth int) bool {
	if depth > 32 {
		return false
	}
	for k, v := range s {
		switch k {
		case "type":
			switch t := v.(type) {
			case string:
				if t != "object" && t != "array" && t != "string" && t != "number" && t != "integer" && t != "boolean" && t != "null" {
					return false
				}
			case []any:
				if len(t) == 0 || len(t) > 7 {
					return false
				}
				for _, item := range t {
					if name, ok := item.(string); !ok || !schemaType(name) {
						return false
					}
				}
			default:
				return false
			}
		case "properties":
			m, ok := v.(map[string]any)
			if !ok {
				return false
			}
			for _, child := range m {
				m, ok := child.(map[string]any)
				if !ok || !validSchemaValue(m, depth+1) {
					return false
				}
			}
		case "required":
			a, ok := v.([]any)
			if !ok {
				return false
			}
			seen := map[string]bool{}
			for _, item := range a {
				name, ok := item.(string)
				if !ok || seen[name] {
					return false
				}
				seen[name] = true
			}
		case "additionalProperties":
			if _, ok := v.(bool); !ok {
				m, ok := v.(map[string]any)
				if !ok || !validSchemaValue(m, depth+1) {
					return false
				}
			}
		case "items":
			m, ok := v.(map[string]any)
			if !ok || !validSchemaValue(m, depth+1) {
				return false
			}
		case "anyOf", "oneOf", "allOf":
			a, ok := v.([]any)
			if !ok || len(a) == 0 {
				return false
			}
			for _, item := range a {
				m, ok := item.(map[string]any)
				if !ok || !validSchemaValue(m, depth+1) {
					return false
				}
			}
		case "enum":
			a, ok := v.([]any)
			if !ok || len(a) == 0 {
				return false
			}
		case "const":
		case "description", "title", "format", "pattern":
			if _, ok := v.(string); !ok {
				return false
			}
			if k == "pattern" {
				if _, err := regexp.Compile(v.(string)); err != nil {
					return false
				}
			}
			if k == "format" {
				f := v.(string)
				if f != "date-time" && f != "date" && f != "uuid" && f != "email" && f != "uri" {
					return false
				}
			}
		case "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum", "multipleOf", "minItems", "maxItems", "minLength", "maxLength", "minProperties", "maxProperties":
			n, ok := v.(float64)
			if !ok || math.IsNaN(n) || math.IsInf(n, 0) {
				return false
			}
		case "uniqueItems":
			if _, ok := v.(bool); !ok {
				return false
			}
		default:
			return false
		}
	}
	return true
}
func ValidateAgentInvocation(in AgentInvocation) error {
	if !ValidPurpose(string(in.Purpose)) || !ValidHex(in.CapabilityRevision, 32) || !trust.ValidID(in.AttemptID) || !validClasses(in.DataClasses) || strings.TrimSpace(in.Instructions) == "" || len(in.Instructions) > 9216 || !utf8.ValidString(in.Instructions) || in.MaxOutputBytes < 1 || in.MaxOutputBytes > 16384 {
		return Failure{Code: Validation}
	}
	encoded, err := json.Marshal(in.Input)
	if err != nil || len(encoded) > 32768 || len(in.Input.Messages) < 1 || len(in.Input.Messages) > 256 || in.Input.Tools == nil || len(in.Input.Tools) > 64 {
		return Failure{Code: Validation}
	}
	names := map[string]bool{}
	for _, t := range in.Input.Tools {
		f := t.Function
		if t.Type != "function" || !ValidAlias(f.Name) || names[f.Name] || f.Strict || !ValidSchema(f.Parameters) {
			return Failure{Code: Validation}
		}
		names[f.Name] = true
	}
	pending, seen := map[string]bool{}, map[string]bool{}
	for _, m := range in.Input.Messages {
		if m.Role == "tool" {
			if m.Content == nil || len(m.ToolCalls) != 0 || !pending[m.ToolCallID] {
				return Failure{Code: Validation}
			}
			delete(pending, m.ToolCallID)
			continue
		}
		if len(pending) != 0 || m.Role != "user" && m.Role != "assistant" || m.ToolCallID != "" {
			return Failure{Code: Validation}
		}
		if len(m.ToolCalls) == 0 {
			if m.Content == nil {
				return Failure{Code: Validation}
			}
			continue
		}
		if m.Role != "assistant" || len(m.ToolCalls) > 8 {
			return Failure{Code: Validation}
		}
		for _, c := range m.ToolCalls {
			if !validCallID(c.ID) || seen[c.ID] || c.Type != "function" || !ValidAlias(c.Function.Name) || !validObject([]byte(c.Function.Arguments)) {
				return Failure{Code: Validation}
			}
			seen[c.ID] = true
			pending[c.ID] = true
		}
	}
	if len(pending) != 0 {
		return Failure{Code: Validation}
	}
	return nil
}
func ValidateStructuredInvocation(in StructuredInvocation) error {
	if !ValidPurpose(string(in.Purpose)) || !ValidHex(in.CapabilityRevision, 32) || !trust.ValidID(in.AttemptID) || !validClasses(in.DataClasses) || strings.TrimSpace(in.Instructions) == "" || len(in.Instructions) > 8192 || !utf8.ValidString(in.Instructions) || in.MaxOutputBytes < 1 || in.MaxOutputBytes > 32768 || !validObject(in.Input) || !ValidSchema(in.OutputSchema) {
		return Failure{Code: Validation}
	}
	return nil
}
func ValidateAgentResult(in AgentInvocation, out AgentResult) error {
	if len(out.Output) < 1 || len(out.Output) > 16 || out.CallIDs == nil {
		return Failure{Code: InvalidOutput}
	}
	data, err := json.Marshal(out.Output)
	if err != nil || uint64(len(data)) > in.MaxOutputBytes {
		return Failure{Code: InvalidOutput}
	}
	catalog := map[string]bool{}
	for _, t := range in.Input.Tools {
		catalog[t.Function.Name] = true
	}
	count := 0
	for _, s := range out.Output {
		switch s.Kind {
		case "preamble", "answer":
			if strings.TrimSpace(s.Text) == "" || !utf8.ValidString(s.Text) || s.CapabilityID != "" || s.Input != "" {
				return Failure{Code: InvalidOutput}
			}
		case "call":
			if !catalog[s.CapabilityID] || s.Text != "" || !validObject([]byte(s.Input)) {
				return Failure{Code: InvalidOutput}
			}
			count++
		default:
			return Failure{Code: InvalidOutput}
		}
	}
	if len(out.CallIDs) != count {
		return Failure{Code: InvalidOutput}
	}
	seen := map[string]bool{}
	for _, id := range out.CallIDs {
		if !validCallID(id) || seen[id] {
			return Failure{Code: InvalidOutput}
		}
		seen[id] = true
	}
	return ValidateUsage(out.Usage)
}
func ValidateUsage(u UsageObservation) error {
	if u.Tokens != nil && *u.Tokens > trust.MaxJSONInteger || u.CostMicros != nil && *u.CostMicros > trust.MaxJSONInteger {
		return Failure{Code: InvalidOutput}
	}
	return nil
}

// ValidateStructuredOutput applies the same bounded provider-neutral schema accepted on input.
func ValidateStructuredOutput(in StructuredInvocation, out json.RawMessage) error {
	if uint64(len(out)) > in.MaxOutputBytes || !validObject(out) {
		return Failure{Code: InvalidOutput}
	}
	var schema map[string]any
	var value any
	if json.Unmarshal(in.OutputSchema, &schema) != nil || json.Unmarshal(out, &value) != nil || !matchesSchema(value, schema) {
		return Failure{Code: InvalidOutput}
	}
	return nil
}
func matchesSchema(v any, s map[string]any) bool {
	if t, ok := s["type"].(string); ok && !matchesType(v, t) {
		return false
	}
	if types, ok := s["type"].([]any); ok {
		matched := false
		for _, t := range types {
			if name, ok := t.(string); ok && matchesType(v, name) {
				matched = true
			}
		}
		if !matched {
			return false
		}
	}
	if expected, ok := s["const"]; ok && !reflectJSON(v, expected) {
		return false
	}
	if choices, ok := s["enum"].([]any); ok {
		match := false
		for _, c := range choices {
			match = match || reflectJSON(v, c)
		}
		if !match {
			return false
		}
	}
	for _, op := range []string{"anyOf", "oneOf", "allOf"} {
		if branches, ok := s[op].([]any); ok {
			n := 0
			for _, b := range branches {
				child, ok := b.(map[string]any)
				if ok && matchesSchema(v, child) {
					n++
				}
			}
			if op == "anyOf" && n == 0 || op == "oneOf" && n != 1 || op == "allOf" && n != len(branches) {
				return false
			}
		}
	}
	switch value := v.(type) {
	case map[string]any:
		properties, _ := s["properties"].(map[string]any)
		if required, ok := s["required"].([]any); ok {
			for _, name := range required {
				if _, exists := value[name.(string)]; !exists {
					return false
				}
			}
		}
		for key, item := range value {
			if raw, ok := properties[key]; ok {
				if !matchesSchema(item, raw.(map[string]any)) {
					return false
				}
			} else {
				switch extra := s["additionalProperties"].(type) {
				case bool:
					if !extra {
						return false
					}
				case map[string]any:
					if !matchesSchema(item, extra) {
						return false
					}
				}
			}
		}
		if !within(float64(len(value)), s, "minProperties", "maxProperties") {
			return false
		}
	case []any:
		if !within(float64(len(value)), s, "minItems", "maxItems") {
			return false
		}
		if item, ok := s["items"].(map[string]any); ok {
			for _, v := range value {
				if !matchesSchema(v, item) {
					return false
				}
			}
		}
		if unique, _ := s["uniqueItems"].(bool); unique {
			seen := map[string]bool{}
			for _, v := range value {
				raw, _ := json.Marshal(v)
				if seen[string(raw)] {
					return false
				}
				seen[string(raw)] = true
			}
		}
	case string:
		if format, ok := s["format"].(string); ok && !validFormat(value, format) {
			return false
		}
		if !within(float64(utf8.RuneCountInString(value)), s, "minLength", "maxLength") {
			return false
		}
		if pattern, ok := s["pattern"].(string); ok {
			r, err := regexp.Compile(pattern)
			if err != nil || !r.MatchString(value) {
				return false
			}
		}
	case float64:
		if !within(value, s, "minimum", "maximum") {
			return false
		}
		if n, ok := s["exclusiveMinimum"].(float64); ok && value <= n {
			return false
		}
		if n, ok := s["exclusiveMaximum"].(float64); ok && value >= n {
			return false
		}
		if n, ok := s["multipleOf"].(float64); ok && (n <= 0 || math.Mod(value, n) != 0) {
			return false
		}
	}
	return true
}
func within(n float64, s map[string]any, min, max string) bool {
	if v, ok := s[min].(float64); ok && n < v {
		return false
	}
	if v, ok := s[max].(float64); ok && n > v {
		return false
	}
	return true
}
func matchesType(v any, t string) bool {
	switch t {
	case "null":
		return v == nil
	case "object":
		_, ok := v.(map[string]any)
		return ok
	case "array":
		_, ok := v.([]any)
		return ok
	case "string":
		_, ok := v.(string)
		return ok
	case "number":
		_, ok := v.(float64)
		return ok
	case "integer":
		n, ok := v.(float64)
		return ok && math.Trunc(n) == n
	case "boolean":
		_, ok := v.(bool)
		return ok
	}
	return false
}
func reflectJSON(a, b any) bool {
	x, e := json.Marshal(a)
	y, f := json.Marshal(b)
	return e == nil && f == nil && bytes.Equal(x, y)
}

func schemaType(s string) bool {
	return s == "object" || s == "array" || s == "string" || s == "number" || s == "integer" || s == "boolean" || s == "null"
}
func validFormat(value, format string) bool {
	switch format {
	case "uuid":
		return trust.ValidID(value)
	case "date-time":
		_, err := time.Parse(time.RFC3339Nano, value)
		return err == nil
	case "date":
		_, err := time.Parse("2006-01-02", value)
		return err == nil
	case "email":
		a, err := mail.ParseAddress(value)
		return err == nil && a.Address == value
	case "uri":
		u, err := url.ParseRequestURI(value)
		return err == nil && u.IsAbs()
	}
	return false
}
