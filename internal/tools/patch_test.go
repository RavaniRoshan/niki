package tools

import (
	"testing"
)

func TestParseAndApplyPatch(t *testing.T) {
	orig := "line 1\nline 2\nline 3\nline 4\n"
	diff := `--- a/file.txt
+++ b/file.txt
@@ -1,4 +1,4 @@
 line 1
-line 2
+line 2 updated
 line 3
 line 4
`
	patches, err := ParsePatch(diff)
	if err != nil {
		t.Fatalf("ParsePatch error: %v", err)
	}
	if len(patches) != 1 {
		t.Fatalf("expected 1 patch, got %d", len(patches))
	}
	if len(patches[0].Hunks) != 1 {
		t.Fatalf("expected 1 hunk, got %d", len(patches[0].Hunks))
	}

	res, err := ApplyPatch(orig, patches[0].Hunks)
	if err != nil {
		t.Fatalf("ApplyPatch error: %v", err)
	}
	expected := "line 1\nline 2 updated\nline 3\nline 4\n"
	if res != expected {
		t.Fatalf("result mismatch:\ngot:\n%s\nwant:\n%s", res, expected)
	}
}
