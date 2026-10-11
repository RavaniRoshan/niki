package index

import (
	"fmt"
	"sort"
	"strings"
)

// Map renders a repository map (C4): a symbol outline grouped by
// file — Go symbols from the AST, other languages from the regex
// fallback — cut to fit tokenBudget (4-chars-per-token estimate).
// The footer always reports coverage, so a truncated map is
// explicit, never silent.
func (idx *SymbolIndex) Map(tokenBudget int) string {
	if tokenBudget <= 0 {
		tokenBudget = 2000
	}
	// Reserve room for the coverage footer so the total,
	// footer included, respects the budget.
	charBudget := tokenBudget*4 - 160
	if charBudget < 160 {
		charBudget = 160
	}
	idx.mu.RLock()
	syms := append([]Symbol{}, idx.symbols...)
	idx.mu.RUnlock()

	byFile := map[string][]Symbol{}
	for _, s := range syms {
		byFile[s.File] = append(byFile[s.File], s)
	}
	files := make([]string, 0, len(byFile))
	for f := range byFile {
		files = append(files, f)
	}
	sort.Strings(files)

	var b strings.Builder
	shown, totalSyms := 0, len(syms)
	for _, f := range files {
		names := make([]string, 0, len(byFile[f]))
		for _, s := range byFile[f] {
			names = append(names, s.Name)
		}
		line := fmt.Sprintf("%s: %s\n", f, strings.Join(names, ", "))
		if b.Len()+len(line) > charBudget {
			break
		}
		b.WriteString(line)
		shown += len(names)
	}
	fmt.Fprintf(&b, "-- map: %d/%d symbols, %d files", shown, totalSyms, len(files))
	if shown < totalSyms {
		fmt.Fprintf(&b, " (truncated to %d tokens)", tokenBudget)
	}
	return b.String()
}
