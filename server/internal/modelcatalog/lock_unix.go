//go:build darwin || linux

package modelcatalog

import (
	"errors"
	"os"
	"path/filepath"
	"syscall"
	"time"

	"floe/server/internal/adapters/storage/privatefiles"
)

const catalogLockTimeout = 10 * time.Second

func lockCatalogFile(path string) (func(), error) {
	directory := filepath.Dir(path)
	if err := storage.PrivateDirectory(directory); err != nil {
		return nil, err
	}
	lockPath := path + ".lock"
	fd, err := syscall.Open(lockPath, syscall.O_CREAT|syscall.O_RDWR|syscall.O_CLOEXEC|syscall.O_NOFOLLOW, 0600)
	if err != nil {
		return nil, err
	}
	file := os.NewFile(uintptr(fd), lockPath)
	info, err := file.Stat()
	if err != nil {
		file.Close()
		return nil, err
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 || int(stat.Uid) != os.Geteuid() || stat.Nlink != 1 {
		file.Close()
		return nil, storage.ErrUnsafePrivateFile
	}
	deadline := time.Now().Add(catalogLockTimeout)
	for {
		err = syscall.Flock(fd, syscall.LOCK_EX|syscall.LOCK_NB)
		if err == nil {
			return func() {
				_ = syscall.Flock(fd, syscall.LOCK_UN)
				_ = file.Close()
			}, nil
		}
		if !errors.Is(err, syscall.EWOULDBLOCK) && !errors.Is(err, syscall.EAGAIN) {
			file.Close()
			return nil, err
		}
		if time.Now().After(deadline) {
			file.Close()
			return nil, errors.New("model catalog writer lock timeout")
		}
		time.Sleep(10 * time.Millisecond)
	}
}
