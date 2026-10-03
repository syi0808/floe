//go:build !darwin || !cgo

package credentials

func nativeKeychain(string, string, int) (string, error) { return "", ErrUnavailable }
