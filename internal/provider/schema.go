package provider

import (
	"encoding/json"
)

// SchemaToJSONSchema converts NIKI's minimal tool schema
// ({"required":[...],"fields":{"name":"string|number|boolean"}}) into a JSON
// Schema object suitable for provider `tools` parameters.
func SchemaToJSONSchema(minimal string) json.RawMessage {
	type minimalSchema struct {
		Required []string          `json:"required"`
		Fields   map[string]string `json:"fields"`
	}
	var ms minimalSchema
	out := map[string]any{
		"type":       "object",
		"properties": map[string]any{},
	}
	if minimal != "" {
		if err := json.Unmarshal([]byte(minimal), &ms); err != nil {
			return json.RawMessage(`{"type":"object","properties":{}}`)
		}
	} else {
		return json.RawMessage(`{"type":"object","properties":{}}`)
	}
	props := out["properties"].(map[string]any)
	for name, typ := range ms.Fields {
		switch typ {
		case "number":
			props[name] = map[string]any{"type": "number"}
		case "boolean":
			props[name] = map[string]any{"type": "boolean"}
		case "object":
			props[name] = map[string]any{"type": "object"}
		case "array":
			props[name] = map[string]any{"type": "array"}
		default:
			props[name] = map[string]any{"type": "string"}
		}
	}
	if len(ms.Required) > 0 {
		out["required"] = ms.Required
	}
	out["additionalProperties"] = false
	enc, err := json.Marshal(out)
	if err != nil {
		return json.RawMessage(`{"type":"object","properties":{}}`)
	}
	return enc
}
