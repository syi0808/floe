// HTTP and admin-UI transport for the local server.

package httptransport

import "embed"

// The admin UI is transport, not business: it renders what owners decided.
//
//go:embed web/*
var assets embed.FS

// Assets exposes the embedded admin UI to the server assembly.
func Assets() embed.FS { return assets }
