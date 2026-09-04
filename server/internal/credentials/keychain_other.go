//go:build !darwin || !cgo

package credentials

import "errors"

type Keychain struct{}

func (Keychain) Get(string) (string, error) {
	return "", errors.New("OS credential store requires macOS and cgo")
}
func (Keychain) Put(string, string) error {
	return errors.New("OS credential store requires macOS and cgo")
}
func (Keychain) Delete(string) error { return errors.New("OS credential store requires macOS and cgo") }
