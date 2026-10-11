package tools

import (
	"context"
	"encoding/json"
	"testing"
)

// update_plan rejects new plans while the session is in plan mode
// (moved from permissions: tools importing permissions for the G3
// push guard forbids the reverse import in tests).
func TestUpdatePlanRejectedInPlanMode(t *testing.T) {
	up := NewUpdatePlanTool()
	up.SetPlanMode(true)
	res, err := up.Run(context.Background(), json.RawMessage(`{
		"steps": [{"title": "Step 1", "status": "in_progress"}]
	}`))
	if err != nil {
		t.Fatal(err)
	}
	if !res.IsError {
		t.Fatalf("update_plan should be rejected in plan mode: %v", res)
	}
}
