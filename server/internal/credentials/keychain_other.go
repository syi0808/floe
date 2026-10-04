//go:build (!darwin || !cgo) && !floe_dev

package credentials

func nativeKeychain(string, string, int) (string, error) { return "", ErrUnavailable }
