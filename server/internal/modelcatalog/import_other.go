//go:build !darwin && !linux

package modelcatalog

func readExternalBounded(string) ([]byte, error) {
	return nil, ErrUnsupportedCatalogPlatform
}
