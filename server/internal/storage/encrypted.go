package storage

import (
	"bytes"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"encoding/binary"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

const maxPayload = 16 * 1024 * 1024

var envelope = []byte{'F', 'L', 'O', 'E', 0, 'E', 'N', 'C', 1}
var ErrUnavailable = errors.New("encrypted storage unavailable")

type fileRoot struct {
	mu                            sync.RWMutex
	path, identity                string
	aead                          cipher.AEAD
	writable, unavailable, closed bool
}

// Files is a scoped encrypted storage capability, not a schema or business owner.
// Only composition creates the root after selecting the compiled key custodian.
type Files struct {
	root  *fileRoot
	scope string
}

func NewFiles(directory, identity string, key []byte, writable bool) (*Files, error) {
	if identity == "" || len(identity) > 128 || len(key) != 32 || !filepath.IsAbs(directory) {
		return nil, ErrUnavailable
	}
	if err := PrivateDirectory(directory); err != nil {
		return nil, err
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, ErrUnavailable
	}
	aead, err := cipher.NewGCM(block)
	if err != nil {
		return nil, ErrUnavailable
	}
	return &Files{root: &fileRoot{path: directory, identity: identity, aead: aead, writable: writable}}, nil
}
func validPart(name string) bool {
	if name == "" || name == "." || name == ".." || len(name) > 256 {
		return false
	}
	for _, c := range name {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_' || c == '.') {
			return false
		}
	}
	return true
}
func (f *Files) ready(write bool) error {
	if f == nil || f.root == nil || f.root.closed || f.root.unavailable || write && !f.root.writable {
		return ErrUnavailable
	}
	if err := PrivateDirectory(f.root.path); err != nil {
		return err
	}
	current := f.root.path
	if f.scope != "" {
		for _, part := range strings.Split(f.scope, "/") {
			current = filepath.Join(current, part)
			if err := PrivateDirectory(current); err != nil {
				return err
			}
		}
	}
	return nil
}
func (f *Files) logical(name string) (string, error) {
	if !validPart(name) {
		return "", ErrUnavailable
	}
	if f.scope == "" {
		return name, nil
	}
	return f.scope + "/" + name, nil
}
func (f *Files) aad(logical string) []byte {
	out := append([]byte{}, envelope...)
	for _, s := range []string{f.root.identity, logical} {
		out = binary.BigEndian.AppendUint64(out, uint64(len(s)))
		out = append(out, s...)
	}
	return out
}
func (f *Files) Read(name string, limit int64) ([]byte, error) {
	if f == nil || f.root == nil {
		return nil, ErrUnavailable
	}
	f.root.mu.RLock()
	defer f.root.mu.RUnlock()
	if err := f.ready(false); err != nil {
		return nil, err
	}
	logical, err := f.logical(name)
	if err != nil || limit < 0 || limit > maxPayload {
		return nil, ErrUnavailable
	}
	extra := len(envelope) + f.root.aead.NonceSize() + f.root.aead.Overhead()
	data, err := ReadPrivate(filepath.Join(f.root.path, filepath.FromSlash(logical)), limit+int64(extra))
	if err != nil {
		return nil, err
	}
	if len(data) < extra || !bytes.Equal(data[:len(envelope)], envelope) {
		return nil, ErrUnavailable
	}
	nonce := data[len(envelope) : len(envelope)+f.root.aead.NonceSize()]
	plaintext, err := f.root.aead.Open(nil, nonce, data[len(envelope)+len(nonce):], f.aad(logical))
	if err != nil || int64(len(plaintext)) > limit {
		return nil, ErrUnavailable
	}
	return plaintext, nil
}
func (f *Files) Write(name string, data []byte) error {
	if f == nil || f.root == nil {
		return ErrUnavailable
	}
	f.root.mu.Lock()
	defer f.root.mu.Unlock()
	if err := f.ready(true); err != nil {
		return err
	}
	logical, err := f.logical(name)
	if err != nil || len(data) > maxPayload {
		return ErrUnavailable
	}
	path := filepath.Join(f.root.path, filepath.FromSlash(logical))
	prior, readErr := ReadPrivate(path, maxPayload+64)
	if readErr != nil && !os.IsNotExist(readErr) {
		return readErr
	}
	if readErr == nil {
		extra := len(envelope) + f.root.aead.NonceSize() + f.root.aead.Overhead()
		if len(prior) < extra || !bytes.Equal(prior[:len(envelope)], envelope) {
			return ErrUnavailable
		}
		n := prior[len(envelope) : len(envelope)+f.root.aead.NonceSize()]
		if _, err := f.root.aead.Open(nil, n, prior[len(envelope)+len(n):], f.aad(logical)); err != nil {
			return ErrUnavailable
		}
	}
	nonce := make([]byte, f.root.aead.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return ErrUnavailable
	}
	encoded := append(append([]byte{}, envelope...), nonce...)
	encoded = f.root.aead.Seal(encoded, nonce, data, f.aad(logical))
	err = WritePrivate(path, encoded)
	if IsIndeterminate(err) {
		f.root.unavailable = true
	}
	return err
}
func (f *Files) Exists(name string) (bool, error) {
	if f == nil || f.root == nil {
		return false, ErrUnavailable
	}
	f.root.mu.RLock()
	defer f.root.mu.RUnlock()
	if err := f.ready(false); err != nil {
		return false, err
	}
	logical, err := f.logical(name)
	if err != nil {
		return false, err
	}
	_, err = ReadPrivate(filepath.Join(f.root.path, filepath.FromSlash(logical)), maxPayload+64)
	if os.IsNotExist(err) {
		return false, nil
	}
	return err == nil, err
}
func (f *Files) Scope(parts ...string) (*Files, error) {
	if f == nil || f.root == nil {
		return nil, ErrUnavailable
	}
	f.root.mu.Lock()
	defer f.root.mu.Unlock()
	if err := f.ready(false); err != nil {
		return nil, err
	}
	scope := f.scope
	path := filepath.Join(f.root.path, filepath.FromSlash(scope))
	for _, part := range parts {
		if !validPart(part) {
			return nil, ErrUnavailable
		}
		path = filepath.Join(path, part)
		if f.root.writable {
			err := os.Mkdir(path, 0700)
			if err != nil && !os.IsExist(err) {
				return nil, err
			}
			if err == nil {
				parent, e := os.Open(filepath.Dir(path))
				if e != nil {
					return nil, e
				}
				e = parent.Sync()
				closeErr := parent.Close()
				if e != nil || closeErr != nil {
					f.root.unavailable = true
					return nil, ErrUnavailable
				}
			}
		}
		if err := PrivateDirectory(path); err != nil {
			return nil, err
		}
		if scope == "" {
			scope = part
		} else {
			scope += "/" + part
		}
	}
	return &Files{root: f.root, scope: scope}, nil
}
func (f *Files) Close() {
	if f == nil || f.root == nil {
		return
	}
	f.root.mu.Lock()
	defer f.root.mu.Unlock()
	f.root.closed = true
	f.root.aead = nil
}
