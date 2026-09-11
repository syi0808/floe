package oauthclients

import (
	_ "embed"
	"strings"
)

//go:embed .env
var bundledEnvironment string

func ClientID(name string) string {
	return parse(bundledEnvironment)[name]
}

func parse(environment string) map[string]string {
	values := make(map[string]string)
	for line := range strings.Lines(environment) {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		name, value, found := strings.Cut(line, "=")
		if found && strings.HasSuffix(strings.TrimSpace(name), "_OAUTH_CLIENT_ID") {
			values[strings.TrimSpace(name)] = strings.TrimSpace(value)
		}
	}
	return values
}
