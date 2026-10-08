//go:build !darwin && !linux

package modelcatalog

func lockCatalogFile(string) (func(), error) {
	return nil, ErrUnsupportedCatalogPlatform
}
