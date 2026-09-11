package envfile

import (
	"errors"
	"fmt"
	"os"
	"strings"
)

const maxSize = 64 * 1024

func Load() error {
	path, explicit := os.LookupEnv("FLOE_ENV_FILE")
	if strings.TrimSpace(path) == "" {
		path = ".env"
		explicit = false
	}
	contents, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) && !explicit {
		return nil
	}
	if err != nil {
		return fmt.Errorf("read environment file: %w", err)
	}
	if len(contents) > maxSize {
		return errors.New("environment file exceeds 64 KiB")
	}
	values, err := parse(string(contents))
	if err != nil {
		return err
	}
	for name, value := range values {
		if _, exists := os.LookupEnv(name); !exists {
			if err := os.Setenv(name, value); err != nil {
				return fmt.Errorf("set environment value %s: %w", name, err)
			}
		}
	}
	return nil
}

func parse(contents string) (map[string]string, error) {
	values := make(map[string]string)
	for index, rawLine := range strings.Split(contents, "\n") {
		line := strings.TrimSpace(strings.TrimSuffix(rawLine, "\r"))
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		line = strings.TrimSpace(strings.TrimPrefix(line, "export "))
		name, value, found := strings.Cut(line, "=")
		name, value = strings.TrimSpace(name), strings.TrimSpace(value)
		if !found || !validName(name) {
			return nil, fmt.Errorf("invalid environment file line %d", index+1)
		}
		if len(value) >= 2 && (value[0] == '\'' && value[len(value)-1] == '\'' || value[0] == '"' && value[len(value)-1] == '"') {
			value = value[1 : len(value)-1]
		}
		values[name] = value
	}
	return values, nil
}

func validName(name string) bool {
	if name == "" || name[0] != '_' && (name[0] < 'A' || name[0] > 'Z') && (name[0] < 'a' || name[0] > 'z') {
		return false
	}
	for _, character := range name[1:] {
		if character != '_' && (character < 'A' || character > 'Z') && (character < 'a' || character > 'z') && (character < '0' || character > '9') {
			return false
		}
	}
	return true
}
