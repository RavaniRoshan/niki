import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

// P1 mirror schema: run headers + stage transitions + coarse events.
// Local `.niki/tasks/<uuid>/` remains the source of truth; these tables are a
// best-effort async mirror for dashboard/history. No logs, blobs, or secrets.
export default defineSchema({
  runs: defineTable({
    taskUuid: v.string(),
    branch: v.optional(v.string()),
    status: v.union(
      v.literal("queued"),
      v.literal("planning"),
      v.literal("coding"),
      v.literal("testing"),
      v.literal("reviewing"),
      v.literal("revision_required"),
      v.literal("approved"),
      v.literal("committed"),
      v.literal("failed"),
      v.literal("cancelled")
    ),
    verdict: v.optional(v.string()),
    rounds: v.number(),
    topology: v.optional(v.string()),
    risk: v.optional(v.string()),
    inputTokens: v.number(),
    outputTokens: v.number(),
    costUsd: v.number(),
    updatedAt: v.number(),
  })
    .index("by_taskUuid", ["taskUuid"])
    .index("by_status", ["status"]),

  stages: defineTable({
    runId: v.id("runs"),
    seq: v.number(),
    role: v.string(),
    attempt: v.number(),
    status: v.string(),
    inputTokens: v.number(),
    outputTokens: v.number(),
    costUsd: v.number(),
    latencyMs: v.number(),
  }).index("by_run_seq", ["runId", "seq"]),

  events: defineTable({
    runId: v.id("runs"),
    seq: v.number(),
    kind: v.string(),
    message: v.string(),
    createdAt: v.number(),
  }).index("by_run", ["runId"]),
});
