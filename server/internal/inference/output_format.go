package inference

import (
	"bytes"
	"encoding/json"
	"math/big"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"

	"floe/server/internal/trust"
)

const maxOutputSchemaBytes = 16384

// DecodeOutputFormat accepts the same closed tagged union as the neutral model
// contract. It deliberately does not reuse the operator /v1/generate schema.
func DecodeOutputFormat(raw []byte) (OutputFormat, error) {
	bad := func() (OutputFormat, error) { return OutputFormat{}, Failure{Code: Validation} }
	if trust.StrictJSON(raw, maxOutputSchemaBytes+128, 32) != nil || !ValidJSONTextEncoding(raw) {
		return bad()
	}
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || fields == nil {
		return bad()
	}
	var kind string
	if json.Unmarshal(fields["kind"], &kind) != nil {
		return bad()
	}
	out := OutputFormat{Kind: kind}
	switch kind {
	case "text":
		if len(fields) != 1 { return bad() }
	case "json":
		if len(fields) != 2 || !ValidOutputSchema(fields["schema"]) { return bad() }
		out.Schema = append(json.RawMessage(nil), fields["schema"]...)
	default:
		return bad()
	}
	return out, nil
}

func validOutputFormat(format OutputFormat) bool {
	return format.Kind == "text" && len(format.Schema) == 0 ||
		format.Kind == "json" && ValidOutputSchema(format.Schema)
}

// The first product message is the canonical run frame. The fixed operator
// probe has no run frame and never enters this paired transport validation.
func ValidateAgentOutputFrame(in AgentInvocation) error {
	if len(in.Input.Messages) == 0 || in.Input.Messages[0].Role != "user" || in.Input.Messages[0].Content == nil {
		return Failure{Code: Validation}
	}
	raw := []byte(*in.Input.Messages[0].Content)
	if trust.StrictJSON(raw, 32768, 32) != nil { return Failure{Code: Validation} }
	var frame map[string]json.RawMessage
	var instructions map[string]json.RawMessage
	if json.Unmarshal(raw, &frame) != nil || json.Unmarshal(frame["run_instructions"], &instructions) != nil {
		return Failure{Code: Validation}
	}
	format, err := DecodeOutputFormat(instructions["output_format"])
	if err != nil || format.Kind != in.OutputFormat.Kind { return Failure{Code: Validation} }
	if format.Kind == "json" {
		expected, validExpected := decodePortableJSON(in.OutputFormat.Schema, maxOutputSchemaBytes)
		observed, validObserved := decodePortableJSON(format.Schema, maxOutputSchemaBytes)
		if !validExpected || !validObserved || !reflectJSON(expected, observed) { return Failure{Code: Validation} }
	}
	return nil
}

func decodePortableJSON(raw []byte, limit int) (any, bool) {
	if trust.StrictJSON(raw, limit, 32) != nil || !ValidJSONTextEncoding(raw) { return nil, false }
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	var value any
	if decoder.Decode(&value) != nil { return nil, false }
	return value, validPortableNumbers(value)
}

// encoding/json replaces malformed UTF-16 escapes. Validate their source
// spelling before decoding so schemas and model content never acquire a
// replacement character that the provider did not actually return.
func ValidJSONTextEncoding(raw []byte) bool {
	if !utf8.Valid(raw) { return false }
	quoted := false
	for index := 0; index < len(raw); index++ {
		if raw[index] == '"' { quoted = !quoted; continue }
		if !quoted || raw[index] != '\\' { continue }
		index++
		if index >= len(raw) { return false }
		if raw[index] != 'u' { continue }
		if index+4 >= len(raw) { return false }
		unit, ok := unicodeEscape(raw[index+1:index+5])
		if !ok { return false }
		index += 4
		if unit >= 0xdc00 && unit <= 0xdfff { return false }
		if unit >= 0xd800 && unit <= 0xdbff {
			if index+6 >= len(raw) || raw[index+1] != '\\' || raw[index+2] != 'u' { return false }
			low, ok := unicodeEscape(raw[index+3:index+7])
			if !ok || low < 0xdc00 || low > 0xdfff { return false }
			index += 6
		}
	}
	return !quoted
}

func unicodeEscape(raw []byte) (uint16, bool) {
	var value uint16
	for _, digit := range raw {
		value *= 16
		switch {
		case digit >= '0' && digit <= '9': value += uint16(digit-'0')
		case digit >= 'a' && digit <= 'f': value += uint16(digit-'a'+10)
		case digit >= 'A' && digit <= 'F': value += uint16(digit-'A'+10)
		default: return 0, false
		}
	}
	return value, true
}

func ValidOutputSchema(raw []byte) bool {
	value, ok := decodePortableJSON(raw, maxOutputSchemaBytes)
	root, object := value.(map[string]any)
	if !ok || !object || root["type"] != "object" { return false }
	nodes := 0
	return validOutputSchemaNode(root, 0, &nodes)
}

func outputSchemaType(schema map[string]any) (string, bool) {
	if value, ok := schema["type"].(string); ok {
		return value, value != "null" && schemaType(value)
	}
	_, typed := schema["type"]
	_, constant := schema["const"]
	return "", !typed && constant
}

func validOutputSchemaNode(schema map[string]any, depth int, nodes *int) bool {
	*nodes = *nodes + 1
	if depth > 8 || *nodes > 256 { return false }
	kind, ok := outputSchemaType(schema)
	if !ok { return false }
	allowed := map[string]bool{"type": true, "const": true, "description": true}
	switch kind {
	case "object":
		allowed["properties"], allowed["required"], allowed["additionalProperties"] = true, true, true
		properties, ok := schema["properties"].(map[string]any)
		required, requiredOK := schema["required"].([]any)
		if !ok || !requiredOK || schema["additionalProperties"] != false || len(properties) > 64 { return false }
		for name, child := range properties {
			value, ok := child.(map[string]any)
			if !validSchemaName(name) || !ok || !validOutputSchemaNode(value, depth+1, nodes) { return false }
		}
		seen := map[string]bool{}
		for _, raw := range required {
			name, ok := raw.(string)
			if !ok || !validSchemaName(name) || seen[name] { return false }
			if _, exists := properties[name]; !exists { return false }
			seen[name] = true
		}
	case "array":
		allowed["items"], allowed["minItems"], allowed["maxItems"] = true, true, true
		child, ok := schema["items"].(map[string]any)
		if !ok || !validOutputSchemaNode(child, depth+1, nodes) || !validOutputBounds(schema, "minItems", "maxItems", true, true) { return false }
	case "string":
		allowed["enum"], allowed["minLength"], allowed["maxLength"] = true, true, true
		if !validOutputBounds(schema, "minLength", "maxLength", true, true) { return false }
		if raw, exists := schema["enum"]; exists {
			values, ok := raw.([]any)
			if !ok || len(values) == 0 || len(values) > 64 { return false }
			seen := map[string]bool{}
			for _, raw := range values {
				value, ok := raw.(string)
				if !ok || seen[value] { return false }
				seen[value] = true
			}
		}
	case "integer", "number":
		allowed["minimum"], allowed["maximum"] = true, true
		if !validOutputBounds(schema, "minimum", "maximum", kind == "integer", false) { return false }
	}
	for key := range schema { if !allowed[key] { return false } }
	if raw, exists := schema["description"]; exists {
		value, ok := raw.(string)
		if !ok || len(value) > 2048 { return false }
	}
	if value, exists := schema["const"]; exists {
		switch value.(type) { case string, bool, json.Number: default: return false }
		if !matchesOutputSchema(value, schema) { return false }
	}
	return true
}

func validSchemaName(value string) bool {
	if value == "" || len(value) > 128 { return false }
	for _, char := range value { if unicode.IsControl(char) { return false } }
	return true
}

func validOutputBounds(schema map[string]any, lower, upper string, integral, sized bool) bool {
	low, high := json.Number("-9007199254740991"), json.Number("9007199254740991")
	for _, key := range []string{lower, upper} {
		if raw, exists := schema[key]; exists {
			number, ok := raw.(json.Number)
			if !ok { return false }
			if integral && !integralJSONNumber(number) || sized && (compareJSONNumbers(number, "0") < 0 || compareJSONNumbers(number, "32768") > 0) { return false }
			if key == lower { low = number } else { high = number }
		}
	}
	return compareJSONNumbers(low, high) <= 0
}

// Preserve mathematical integer semantics without rounding a fractional JSON
// token to an integer through float64 (for example 1.0000000000000001).
func integralJSONNumber(number json.Number) bool {
	value := decimalNumber(number)
	return value.digits == "" || value.point.Cmp(big.NewInt(int64(len(value.digits)))) >= 0
}

// Compare decimal tokens without float rounding or expansion of exponent-sized
// buffers. StrictJSON has already established the bounded JSON number grammar.
type decimalValue struct { negative bool; digits string; point *big.Int }

func decimalNumber(number json.Number) decimalValue {
	raw := number.String()
	negative := strings.HasPrefix(raw, "-")
	raw = strings.TrimPrefix(raw, "-")
	point := new(big.Int)
	if index := strings.IndexAny(raw, "eE"); index >= 0 {
		point.SetString(raw[index+1:], 10)
		raw = raw[:index]
	}
	integerDigits := len(raw)
	if index := strings.IndexByte(raw, '.'); index >= 0 {
		integerDigits = index
		raw = raw[:index]+raw[index+1:]
	}
	digits := strings.TrimLeft(raw, "0")
	point.Add(point, big.NewInt(int64(integerDigits-(len(raw)-len(digits)))))
	digits = strings.TrimRight(digits, "0")
	if digits == "" { return decimalValue{point: new(big.Int)} }
	return decimalValue{negative: negative, digits: digits, point: point}
}

func compareJSONNumbers(left, right json.Number) int {
	a, b := decimalNumber(left), decimalNumber(right)
	if a.negative != b.negative { if a.negative { return -1 }; return 1 }
	result := 0
	if a.digits == "" || b.digits == "" {
		if a.digits != "" { result = 1 } else if b.digits != "" { result = -1 }
	} else if result = a.point.Cmp(b.point); result == 0 {
		width := len(a.digits); if len(b.digits) > width { width = len(b.digits) }
		result = strings.Compare(a.digits+strings.Repeat("0", width-len(a.digits)), b.digits+strings.Repeat("0", width-len(b.digits)))
	}
	if a.negative { return -result }
	return result
}

func validPortableNumbers(value any) bool {
	switch value := value.(type) {
	case json.Number:
		if !validPortableNumber(value) { return false }
	case map[string]any:
		for _, child := range value { if !validPortableNumbers(child) { return false } }
	case []any:
		for _, child := range value { if !validPortableNumbers(child) { return false } }
	}
	return true
}

func validPortableNumber(number json.Number) bool {
	raw := number.String()
	if len(raw) > 64 { return false }
	coefficient := raw
	if index := strings.IndexAny(raw, "eE"); index >= 0 {
		exponent, err := strconv.Atoi(raw[index+1:])
		if err != nil || exponent < -32 || exponent > 32 { return false }
		coefficient = raw[:index]
	}
	digits := 0
	for _, char := range coefficient { if char >= '0' && char <= '9' { digits++ } }
	if digits > 32 { return false }
	parsed, err := number.Float64()
	if err != nil || compareJSONNumbers(number, "-9007199254740991") < 0 || compareJSONNumbers(number, "9007199254740991") > 0 { return false }
	// All runtimes expose canonical doubles. Reject tokens that would silently
	// acquire a different decimal value at that shared boundary.
	return compareJSONNumbers(number, json.Number(strconv.FormatFloat(parsed, 'g', -1, 64))) == 0
}

func ValidateJSONAnswer(format OutputFormat, raw []byte, limit uint64) error {
	if format.Kind != "json" || !ValidOutputSchema(format.Schema) || uint64(len(raw)) > limit { return Failure{Code: InvalidOutput} }
	value, ok := decodePortableJSON(raw, 32768)
	if _, object := value.(map[string]any); !ok || !object { return Failure{Code: InvalidOutput} }
	schema, _ := decodePortableJSON(format.Schema, maxOutputSchemaBytes)
	if !matchesOutputSchema(value, schema.(map[string]any)) { return Failure{Code: InvalidOutput} }
	return nil
}

func matchesOutputSchema(value any, schema map[string]any) bool {
	if constant, exists := schema["const"]; exists && !equalOutputScalar(value, constant) { return false }
	kind, ok := outputSchemaType(schema)
	if !ok { return false }
	if value == nil { return false }
	switch kind {
	case "": return true
	case "object":
		object, ok := value.(map[string]any)
		if !ok { return false }
		properties := schema["properties"].(map[string]any)
		for _, raw := range schema["required"].([]any) { if _, exists := object[raw.(string)]; !exists { return false } }
		for key, item := range object {
			child, exists := properties[key]
			if !exists || !matchesOutputSchema(item, child.(map[string]any)) { return false }
		}
	case "array":
		array, ok := value.([]any)
		if !ok || !outputWithin(json.Number(strconv.Itoa(len(array))), schema, "minItems", "maxItems") { return false }
		for _, item := range array { if !matchesOutputSchema(item, schema["items"].(map[string]any)) { return false } }
	case "string":
		text, ok := value.(string)
		if !ok || !outputWithin(json.Number(strconv.Itoa(utf8.RuneCountInString(text))), schema, "minLength", "maxLength") { return false }
		if values, exists := schema["enum"].([]any); exists {
			found := false
			for _, candidate := range values { found = found || candidate == text }
			if !found { return false }
		}
	case "integer", "number":
		number, ok := value.(json.Number)
		if !ok || kind == "integer" && !integralJSONNumber(number) { return false }
		if !outputWithin(number, schema, "minimum", "maximum") { return false }
	case "boolean":
		if _, ok := value.(bool); !ok { return false }
	default: return false
	}
	return true
}

func outputWithin(value json.Number, schema map[string]any, lower, upper string) bool {
	if number, ok := schema[lower].(json.Number); ok && compareJSONNumbers(value, number) < 0 { return false }
	if number, ok := schema[upper].(json.Number); ok && compareJSONNumbers(value, number) > 0 { return false }
	return true
}

func equalOutputScalar(left, right any) bool {
	if a, ok := left.(json.Number); ok {
		b, ok := right.(json.Number)
		if !ok { return false }
		return compareJSONNumbers(a, b) == 0
	}
	switch left.(type) { case nil, string, bool: return left == right }
	return false
}
