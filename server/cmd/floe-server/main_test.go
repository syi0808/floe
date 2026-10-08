package main

import "testing"

func TestParseCatalogCommand(t *testing.T) {
	tests := []struct {
		name    string
		args    []string
		command string
		path    string
		wantOK  bool
	}{
		{name: "validate", args: []string{"--validate-model-catalog", "input.json"}, command: "validate", path: "input.json", wantOK: true},
		{name: "install", args: []string{"--install-model-catalog", "input.json"}, command: "install", path: "input.json", wantOK: true},
		{name: "rollback", args: []string{"--rollback-model-catalog"}, command: "rollback", wantOK: true},
		{name: "server", args: []string{}, wantOK: true},
		{name: "print token", args: []string{"--print-admin-token"}, wantOK: true},
		{name: "unknown option", args: []string{"--something-else"}, wantOK: false},
		{name: "missing file", args: []string{"--install-model-catalog"}, wantOK: false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			command, path, ok := parseCatalogCommand(test.args)
			if command != test.command || path != test.path || ok != test.wantOK {
				t.Fatalf("parseCatalogCommand(%q) = (%q, %q, %t)", test.args, command, path, ok)
			}
		})
	}
}
