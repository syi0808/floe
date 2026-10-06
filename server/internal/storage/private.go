// Package storage implements bounded private-file mechanics, without business policy.
package storage

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"syscall"
)

// ErrUnsafePrivateFile marks a file or directory that fails the private-file
// metadata checks. Filesystem errors are returned with their original cause so
// the profile owner can distinguish access denial from malformed metadata.
var ErrUnsafePrivateFile = errors.New("unsafe private-file metadata")

type unsafePrivateFileError struct{ cause error }

func (e unsafePrivateFileError) Error() string { return ErrUnsafePrivateFile.Error() }
func (e unsafePrivateFileError) Unwrap() error { return e.cause }
func (e unsafePrivateFileError) Is(target error) bool {
	return target == ErrUnsafePrivateFile
}

type IndeterminateWrite struct{ Cause error }

func (e IndeterminateWrite) Error() string { return "private write durability uncertain" }
func (e IndeterminateWrite) Unwrap() error { return e.Cause }
func IsIndeterminate(err error) bool       { var e IndeterminateWrite; return errors.As(err, &e) }
func PrivateDirectory(path string) error {
	info, err := os.Lstat(path)
	if err != nil {
		return err
	}
	if !info.IsDir() || info.Mode().Perm()&0077 != 0 {
		return ErrUnsafePrivateFile
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(stat.Uid) != os.Geteuid() {
		return ErrUnsafePrivateFile
	}
	return nil
}
func openPrivate(path string, limit int64) (*os.File, error) {
	if limit < 0 {
		return nil, ErrIntegrity
	}
	f, err := os.OpenFile(path, os.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if err != nil {
		if errors.Is(err, syscall.ELOOP) {
			return nil, unsafePrivateFileError{cause: err}
		}
		return nil, err
	}
	info, err := f.Stat()
	if err != nil {
		f.Close()
		return nil, err
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 || !ok || int(stat.Uid) != os.Geteuid() || stat.Nlink != 1 {
		f.Close()
		return nil, ErrUnsafePrivateFile
	}
	if info.Size() > limit {
		f.Close()
		return nil, ErrIntegrity
	}
	return f, nil
}
func ReadPrivate(path string, limit int64) ([]byte, error) {
	f, err := openPrivate(path, limit)
	if err != nil {
		return nil, err
	}
	data, err := io.ReadAll(io.LimitReader(f, limit+1))
	closeErr := f.Close()
	if err != nil {
		return nil, err
	}
	if closeErr != nil {
		return nil, closeErr
	}
	if int64(len(data)) > limit {
		return nil, ErrIntegrity
	}
	return data, nil
}
func WritePrivate(path string, data []byte) error {
	if err := PrivateDirectory(filepath.Dir(path)); err != nil {
		return err
	}
	f, err := os.CreateTemp(filepath.Dir(path), ".floe-*")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	if err = f.Chmod(0600); err != nil {
		f.Close()
		return err
	}
	if _, err = f.Write(data); err != nil {
		f.Close()
		return err
	}
	if err = f.Sync(); err != nil {
		f.Close()
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	if err = os.Rename(f.Name(), path); err != nil {
		return err
	}
	dir, err := os.Open(filepath.Dir(path))
	if err != nil {
		return IndeterminateWrite{err}
	}
	if err = dir.Sync(); err != nil {
		dir.Close()
		return IndeterminateWrite{err}
	}
	if err = dir.Close(); err != nil {
		return IndeterminateWrite{err}
	}
	return nil
}
