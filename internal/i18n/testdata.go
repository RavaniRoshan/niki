package i18n

import "os"

// readFileBytes is a tiny helper for catalog/golden comparison (tests only).
func readFileBytes(path string) ([]byte, error) {
	return os.ReadFile(path)
}
