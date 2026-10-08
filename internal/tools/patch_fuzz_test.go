package tools

import (
	"testing"
)

// FuzzParsePatch ensures arbitrary text or corrupt unified diffs never crash or panic the patch parser.
func FuzzParsePatch(f *testing.F) {
	// Seed corpus
	f.Add(`--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 mod
 line 3
`)
	f.Add(`@@ -0,0 +1,1 @@
+new content
`)
	f.Add(`corrupt header @@ -invalid +ranges @@ not a diff`)
	f.Add(``)
	f.Add(`--- `)
	f.Add(`+++ `)
	f.Add("@@ -1,1 +1,1 @@\n\x00\xff\xfe\n")

	f.Fuzz(func(t *testing.T, data string) {
		patches, err := ParsePatch(data)
		if err == nil {
			for _, p := range patches {
				_, _ = ApplyPatch("sample text\nline 2\nline 3\n", p.Hunks)
			}
		}
	})
}
