import { mutation, query } from "./_generated/server";
import { v } from "convex/values";

// P1 mirror functions. Every write is idempotent on a stable key and uses a
// compare-and-set on the expected predecessor status, so duplicate deliveries
// (reconnect flush, retried HTTP) are safe no-ops. Terminal states are final:
// once `committed`/`failed`/`cancelled`, further transitions are rejected.

const TERMINAL = ["committed", "failed", "cancelled"] as const;

const statusValidator = v.union(
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
);

export const createRun = mutation({
  args: {
    taskUuid: v.string(),
    status: statusValidator,
    topology: v.optional(v.string()),
    risk: v.optional(v.string()),
    now: v.number(),
  },
  handler: async (ctx, args) => {
    const existing = await ctx.db
      .query("runs")
      .withIndex("by_taskUuid", (q) => q.eq("taskUuid", args.taskUuid))
      .unique();
    if (existing !== null) {
      return existing._id;
    }
    return await ctx.db.insert("runs", {
      taskUuid: args.taskUuid,
      status: args.status,
      topology: args.topology,
      risk: args.risk,
      rounds: 0,
      inputTokens: 0,
      outputTokens: 0,
      costUsd: 0,
      updatedAt: args.now,
    });
  },
});

export const transitionRun = mutation({
  args: {
    taskUuid: v.string(),
    expectedStatus: statusValidator,
    nextStatus: statusValidator,
    branch: v.optional(v.string()),
    verdict: v.optional(v.string()),
    rounds: v.optional(v.number()),
    inputTokens: v.optional(v.number()),
    outputTokens: v.optional(v.number()),
    costUsd: v.optional(v.number()),
    now: v.number(),
  },
  handler: async (ctx, args) => {
    const run = await ctx.db
      .query("runs")
      .withIndex("by_taskUuid", (q) => q.eq("taskUuid", args.taskUuid))
      .unique();
    if (run === null) {
      throw new Error(`unknown run ${args.taskUuid}`);
    }
    if ((TERMINAL as readonly string[]).includes(run.status)) {
      return run._id;
    }
    if (run.status !== args.expectedStatus) {
      throw new Error(
        `status conflict: expected ${args.expectedStatus}, found ${run.status}`
      );
    }
    await ctx.db.patch(run._id, {
      status: args.nextStatus,
      branch: args.branch ?? run.branch,
      verdict: args.verdict ?? run.verdict,
      rounds: args.rounds ?? run.rounds,
      inputTokens: args.inputTokens ?? run.inputTokens,
      outputTokens: args.outputTokens ?? run.outputTokens,
      costUsd: args.costUsd ?? run.costUsd,
      updatedAt: args.now,
    });
    return run._id;
  },
});

export const recordStage = mutation({
  args: {
    taskUuid: v.string(),
    seq: v.number(),
    role: v.string(),
    attempt: v.number(),
    status: v.string(),
    inputTokens: v.number(),
    outputTokens: v.number(),
    costUsd: v.number(),
    latencyMs: v.number(),
  },
  handler: async (ctx, args) => {
    const run = await ctx.db
      .query("runs")
      .withIndex("by_taskUuid", (q) => q.eq("taskUuid", args.taskUuid))
      .unique();
    if (run === null) {
      throw new Error(`unknown run ${args.taskUuid}`);
    }
    const existing = await ctx.db
      .query("stages")
      .withIndex("by_run_seq", (q) =>
        q.eq("runId", run._id).eq("seq", args.seq)
      )
      .unique();
    if (existing !== null) {
      await ctx.db.patch(existing._id, {
        status: args.status,
        inputTokens: args.inputTokens,
        outputTokens: args.outputTokens,
        costUsd: args.costUsd,
        latencyMs: args.latencyMs,
      });
      return existing._id;
    }
    return await ctx.db.insert("stages", {
      runId: run._id,
      seq: args.seq,
      role: args.role,
      attempt: args.attempt,
      status: args.status,
      inputTokens: args.inputTokens,
      outputTokens: args.outputTokens,
      costUsd: args.costUsd,
      latencyMs: args.latencyMs,
    });
  },
});

export const getRun = query({
  args: { taskUuid: v.string() },
  handler: async (ctx, args) => {
    const run = await ctx.db
      .query("runs")
      .withIndex("by_taskUuid", (q) => q.eq("taskUuid", args.taskUuid))
      .unique();
    if (run === null) {
      return null;
    }
    const stages = await ctx.db
      .query("stages")
      .withIndex("by_run_seq", (q) => q.eq("runId", run._id))
      .collect();
    return { run, stages };
  },
});
