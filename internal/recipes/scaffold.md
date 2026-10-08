---
name: scaffold
description: Scaffold a Go source file plus its test
match: scaffold | create a new file | new component | add a new package file
steps:
  - write_file: {"path": "{{dir}}/{{name}}.go", "content": "package main\n\n// {{Name}} is a new component.\nfunc {{Name}}() string {\n\treturn \"{{name}}\"\n}\n"}
  - write_file: {"path": "{{dir}}/{{name}}_test.go", "content": "package main\n\nimport \"testing\"\n\nfunc Test{{Name}}(t *testing.T) {\n\tif {{Name}}() != \"{{name}}\" {\n\t\tt.Fatal(\"unexpected value\")\n\t}\n}\n"}
  - shell: {"command": "gofmt -l . && go vet ./... 2>&1 | tail -5", "dir": "{{dir}}"}
---
Creates `<name>.go` and `<name>_test.go`, then format-checks and vets.
Needs `name` and `Name` (exported) vars, e.g. name=worker Name=Worker.
