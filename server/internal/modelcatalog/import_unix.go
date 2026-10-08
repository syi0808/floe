//go:build darwin || linux

package modelcatalog

import (
	"io"
	"os"
	"syscall"
)

func readExternalBounded(path string) ([]byte, error) {
	fd, err := syscall.Open(path, syscall.O_RDONLY|syscall.O_CLOEXEC|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if err != nil {
		return nil, err
	}
	file := os.NewFile(uintptr(fd), path)
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return nil, err
	}
	if !info.Mode().IsRegular() || info.Size() > MaxCatalogBytes {
		return nil, ErrInvalidCatalog
	}
	data, err := io.ReadAll(io.LimitReader(file, MaxCatalogBytes+1))
	if err != nil {
		return nil, err
	}
	if len(data) > MaxCatalogBytes {
		return nil, ErrInvalidCatalog
	}
	if _, err := Parse(data); err != nil {
		return nil, err
	}
	return data, nil
}
