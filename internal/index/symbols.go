package index

import (
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
)

// SymbolKind indicates the category of a parsed code symbol.
type SymbolKind string

const (
	KindFunc      SymbolKind = "func"
	KindType      SymbolKind = "type"
	KindInterface SymbolKind = "interface"
	KindStruct    SymbolKind = "struct"
	KindConst     SymbolKind = "const"
)

// Symbol represents an exported or notable declaration in the workspace.
type Symbol struct {
	Name      string     `json:"name"`
	Kind      SymbolKind `json:"kind"`
	File      string     `json:"file"`
	Line      int        `json:"line"`
	Signature string     `json:"signature"`
}

// SymbolIndex maintains an in-memory index of code symbols.
type SymbolIndex struct {
	mu      sync.RWMutex
	symbols []Symbol
}

// NewSymbolIndex creates an empty symbol index.
func NewSymbolIndex() *SymbolIndex {
	return &SymbolIndex{}
}

// Scan walks the root directory and extracts code symbols.
func (idx *SymbolIndex) Scan(root string) error {
	var collected []Symbol

	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			switch d.Name() {
			case ".git", "node_modules", ".nikicode", ".niki", "bin", "vendor":
				return filepath.SkipDir
			}
			return nil
		}

		rel, _ := filepath.Rel(root, path)
		ext := strings.ToLower(filepath.Ext(path))

		switch ext {
		case ".go":
			collected = append(collected, parseGoFile(path, rel)...)
		case ".py", ".ts", ".js", ".rs":
			collected = append(collected, parseRegexFile(path, rel, ext)...)
		}
		return nil
	})

	if err != nil {
		return err
	}

	idx.mu.Lock()
	idx.symbols = collected
	idx.mu.Unlock()
	return nil
}

func parseGoFile(fullPath, relPath string) []Symbol {
	fset := token.NewFileSet()
	node, err := parser.ParseFile(fset, fullPath, nil, parser.ParseComments)
	if err != nil {
		return nil
	}

	var syms []Symbol
	for _, decl := range node.Decls {
		switch d := decl.(type) {
		case *ast.FuncDecl:
			pos := fset.Position(d.Pos())
			kind := KindFunc
			sig := d.Name.Name
			if d.Recv != nil && len(d.Recv.List) > 0 {
				sig = "method " + d.Name.Name
			}
			syms = append(syms, Symbol{
				Name:      d.Name.Name,
				Kind:      kind,
				File:      relPath,
				Line:      pos.Line,
				Signature: sig,
			})

		case *ast.GenDecl:
			pos := fset.Position(d.Pos())
			for _, spec := range d.Specs {
				if ts, ok := spec.(*ast.TypeSpec); ok {
					kind := KindType
					if _, ok := ts.Type.(*ast.InterfaceType); ok {
						kind = KindInterface
					} else if _, ok := ts.Type.(*ast.StructType); ok {
						kind = KindStruct
					}
					syms = append(syms, Symbol{
						Name:      ts.Name.Name,
						Kind:      kind,
						File:      relPath,
						Line:      pos.Line,
						Signature: "type " + ts.Name.Name,
					})
				}
			}
		}
	}
	return syms
}

var (
	pyDefRegex = regexp.MustCompile(`(?m)^(?:def|class)\s+([A-Za-z0-9_]+)`)
	tsFnRegex  = regexp.MustCompile(`(?m)^(?:export\s+)?(?:function|class|interface|type)\s+([A-Za-z0-9_]+)`)
	rsFnRegex  = regexp.MustCompile(`(?m)^(?:pub\s+)?(?:fn|struct|enum|trait)\s+([A-Za-z0-9_]+)`)
)

func parseRegexFile(fullPath, relPath, ext string) []Symbol {
	data, err := os.ReadFile(fullPath)
	if err != nil {
		return nil
	}
	content := string(data)
	lines := strings.Split(content, "\n")

	var re *regexp.Regexp
	switch ext {
	case ".py":
		re = pyDefRegex
	case ".ts", ".js":
		re = tsFnRegex
	case ".rs":
		re = rsFnRegex
	}

	if re == nil {
		return nil
	}

	var syms []Symbol
	for lineNum, line := range lines {
		if m := re.FindStringSubmatch(line); len(m) > 1 {
			name := m[1]
			syms = append(syms, Symbol{
				Name:      name,
				Kind:      KindFunc,
				File:      relPath,
				Line:      lineNum + 1,
				Signature: strings.TrimSpace(line),
			})
		}
	}
	return syms
}

// Search finds symbols matching query with case-insensitive substring ranking.
func (idx *SymbolIndex) Search(query string, limit int) []Symbol {
	idx.mu.RLock()
	defer idx.mu.RUnlock()

	q := strings.ToLower(strings.TrimSpace(query))
	if q == "" {
		if len(idx.symbols) > limit {
			return idx.symbols[:limit]
		}
		return idx.symbols
	}

	type match struct {
		sym   Symbol
		score int
	}

	var matches []match
	for _, s := range idx.symbols {
		low := strings.ToLower(s.Name)
		if low == q {
			matches = append(matches, match{sym: s, score: 100})
		} else if strings.HasPrefix(low, q) {
			matches = append(matches, match{sym: s, score: 80})
		} else if strings.Contains(low, q) {
			matches = append(matches, match{sym: s, score: 50})
		}
	}

	sort.Slice(matches, func(i, j int) bool {
		return matches[i].score > matches[j].score
	})

	var out []Symbol
	for i := 0; i < len(matches) && i < limit; i++ {
		out = append(out, matches[i].sym)
	}
	return out
}

// TopSymbols returns the first N indexed symbols.
func (idx *SymbolIndex) TopSymbols(limit int) []Symbol {
	idx.mu.RLock()
	defer idx.mu.RUnlock()

	if len(idx.symbols) <= limit {
		return idx.symbols
	}
	return idx.symbols[:limit]
}
