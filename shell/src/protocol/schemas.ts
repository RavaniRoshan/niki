/**
 * Runtime validation for everything the engine sends.
 *
 * Types generated from Rust are erased at runtime, so the shell validates inbound frames against
 * zod schemas. A shell that trusted the type system alone would happily render `undefined` when
 * an older or newer engine disagrees with it; this is what makes "the shell handles every
 * declared message" a property of the running program and not of a type-checker.
 *
 * The schemas are keyed by method name, so the test that asserts coverage can walk one table and
 * prove there is no message the shell has no validator for.
 */

import { z } from 'zod';

const severity = z.enum(['info', 'warning', 'error']);
/**
 * Exported so the permission picker and the settings sheet offer exactly the modes the protocol
 * declares, instead of a hand-copied list that can drift from the wire.
 */
export const permissionMode = z.enum(['manual', 'auto', 'dont_ask', 'bypass']);
const provenance = z.enum(['independent', 'self_verification']);
const stageRole = z.enum([
  'planner',
  'coder',
  'tester',
  'reviewer',
  'synthesizer',
  'security_auditor',
  'red',
  'critic',
]);

export const SessionReadySchema = z.object({
  session_id: z.string(),
  project_path: z.string(),
  model: z.string(),
  permission_mode: permissionMode,
  branch: z.string().nullable(),
  ahead: z.number().int().nonnegative().nullable(),
  behind: z.number().int().nonnegative().nullable(),
  resumed_messages: z.number().int().nonnegative(),
});

export const TurnStartedSchema = z.object({
  turn_id: z.string(),
  prompt: z.string(),
});

export const TurnDeltaSchema = z.object({
  turn_id: z.string(),
  text: z.string(),
});

export const TurnEndSchema = z.object({
  turn_id: z.string(),
  summary: z.string(),
  duration_ms: z.number().int().nonnegative(),
  tool_calls: z.number().int().nonnegative(),
  files_changed: z.number().int().nonnegative(),
});

export const StageStartSchema = z.object({
  stage_id: z.string(),
  role: stageRole,
  attempt: z.number().int().positive(),
});

export const StageTokenSchema = z.object({
  stage_id: z.string(),
  role: stageRole,
  text: z.string(),
});

export const StageDoneSchema = z.object({
  stage_id: z.string(),
  role: stageRole,
  summary: z.string(),
  tokens_in: z.number().int().nonnegative(),
  tokens_out: z.number().int().nonnegative(),
  cost_usd: z.number().nonnegative(),
  latency_ms: z.number().int().nonnegative(),
  retry_count: z.number().int().nonnegative(),
  artifact_ref: z.string().nullable(),
  provenance: provenance,
});

export const StageFailedSchema = z.object({
  stage_id: z.string(),
  role: stageRole,
  error: z.string(),
  severity: severity,
  recovery: z.string().nullable(),
});

export const ToolCallSchema = z.object({
  tool_id: z.string(),
  name: z.string(),
  args: z.string(),
});

export const ToolProgressSchema = z.object({
  tool_id: z.string(),
  note: z.string(),
});

export const ToolResultSchema = z.object({
  tool_id: z.string(),
  ok: z.boolean(),
  summary: z.string(),
  full_ref: z.string().nullable(),
  duration_ms: z.number().int().nonnegative(),
});

const diffLine = z.object({
  kind: z.enum(['context', 'added', 'removed']),
  text: z.string(),
});

const hunk = z.object({
  old_start: z.number().int().nonnegative(),
  old_lines: z.number().int().nonnegative(),
  new_start: z.number().int().nonnegative(),
  new_lines: z.number().int().nonnegative(),
  header: z.string(),
  lines: z.array(diffLine),
});

export const ToolDiffSchema = z.object({
  tool_id: z.string(),
  path: z.string(),
  hunks: z.array(hunk),
});

export const ApprovalRequestSchema = z.object({
  id: z.string(),
  tool: z.string(),
  command: z.string(),
  options: z.array(z.object({ id: z.string(), label: z.string() })).nonempty(),
  safest_option_id: z.string(),
});

export const PlanUpdateSchema = z.object({
  items: z.array(z.object({ text: z.string(), done: z.boolean() })),
});

export const NoticeSchema = z.object({ text: z.string(), level: severity });

export const DiffReadySchema = z.object({ ref: z.string() });

export const VerdictReadySchema = z.object({
  ref: z.string(),
  verdict: z.string(),
  provenance: provenance,
});

export const BranchCreatedSchema = z.object({ name: z.string() });

export const ContextUsageSchema = z.object({
  used: z.number().int().nonnegative(),
  limit: z.number().int().nonnegative(),
});

export const CostUpdateSchema = z.object({ usd: z.number().nonnegative() });

export const FinalSchema = z.object({
  verdict: z.string().nullable(),
  error: z.string().nullable(),
});

/** One entry per declared `ServerNotification` variant. No message may lack an entry. */
export const NOTIFICATION_SCHEMAS = {
  'session.ready': SessionReadySchema,
  'turn.started': TurnStartedSchema,
  'turn.delta': TurnDeltaSchema,
  'turn.end': TurnEndSchema,
  'stage.start': StageStartSchema,
  'stage.token': StageTokenSchema,
  'stage.done': StageDoneSchema,
  'stage.failed': StageFailedSchema,
  'tool.call': ToolCallSchema,
  'tool.progress': ToolProgressSchema,
  'tool.result': ToolResultSchema,
  'tool.diff': ToolDiffSchema,
  'approval.request': ApprovalRequestSchema,
  'plan.update': PlanUpdateSchema,
  notice: NoticeSchema,
  'diff.ready': DiffReadySchema,
  'verdict.ready': VerdictReadySchema,
  'branch.created': BranchCreatedSchema,
  'context.usage': ContextUsageSchema,
  'cost.update': CostUpdateSchema,
  final: FinalSchema,
} as const satisfies Record<string, z.ZodTypeAny>;

export type NotificationMethod = keyof typeof NOTIFICATION_SCHEMAS;

/** The methods the shell declares it can handle. The drift test compares this to the Rust source. */
export const HANDLED_METHODS = Object.keys(NOTIFICATION_SCHEMAS) as NotificationMethod[];

const requestResults = {
  initialize: z.object({
    protocol_version: z.number().int().positive(),
    engine_version: z.string(),
    capabilities: z.object({
      streaming: z.boolean(),
      approvals: z.boolean(),
      sessions: z.boolean(),
      diffs: z.boolean(),
      context_usage: z.boolean(),
      cost: z.boolean(),
    }),
  }),
  shutdown: z.object({ ok: z.boolean() }),
  'session.load': z.object({
    session_id: z.string(),
    resumed_messages: z.number().int().nonnegative(),
    project_path: z.string(),
    branch: z.string().nullable(),
  }),
  'turn.start': z.object({ turn_id: z.string() }),
  'approval.reply': z.object({
    id: z.string(),
    decision: z.enum(['allow', 'deny', 'deny_with_reason', 'allow_always']),
  }),
} as const satisfies Record<string, z.ZodTypeAny>;

const rpcError = z.object({
  code: z.number().int(),
  message: z.string(),
  data: z.string().nullable(),
});

const rawResponseSchema = z.union([
  z.object({
    jsonrpc: z.literal('2.0'),
    id: z.number().int(),
    trace_id: z.string().min(1),
    result: z.record(z.unknown()),
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    id: z.number().int(),
    trace_id: z.string().min(1),
    error: rpcError,
  }),
]);

const rawNotificationSchema = z.union([
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('session.ready'),
    params: SessionReadySchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('turn.started'),
    params: TurnStartedSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('turn.delta'),
    params: TurnDeltaSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('turn.end'),
    params: TurnEndSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('stage.start'),
    params: StageStartSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('stage.token'),
    params: StageTokenSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('stage.done'),
    params: StageDoneSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('stage.failed'),
    params: StageFailedSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('tool.call'),
    params: ToolCallSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('tool.progress'),
    params: ToolProgressSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('tool.result'),
    params: ToolResultSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('tool.diff'),
    params: ToolDiffSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('approval.request'),
    params: ApprovalRequestSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('plan.update'),
    params: PlanUpdateSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('notice'),
    params: NoticeSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('diff.ready'),
    params: DiffReadySchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('verdict.ready'),
    params: VerdictReadySchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('branch.created'),
    params: BranchCreatedSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('context.usage'),
    params: ContextUsageSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('cost.update'),
    params: CostUpdateSchema,
  }),
  z.object({
    jsonrpc: z.literal('2.0'),
    trace_id: z.string().min(1),
    method: z.literal('final'),
    params: FinalSchema,
  }),
]);

export type DecodedFrame =
  | { kind: 'notification'; notification: { method: NotificationMethod; params: unknown } }
  | {
      kind: 'response';
      id: number;
      outcome:
        | { result: unknown }
        | { error: { code: number; message: string; data: string | null } };
    };

/**
 * The two schemas the client actually calls, each narrowed into {@link DecodedFrame} so the
 * caller never re-inspects the raw value.
 */
export const notificationSchema = rawNotificationSchema.transform((v): DecodedFrame => ({
  kind: 'notification',
  notification: { method: v.method as NotificationMethod, params: v.params },
}));

export const responseSchema = rawResponseSchema.transform((v): DecodedFrame => {
  if ('error' in v) {
    return { kind: 'response', id: v.id, outcome: { error: v.error } };
  }
  return { kind: 'response', id: v.id, outcome: { result: v.result } };
});

/** Re-exported so callers can build a request payload without importing two modules. */
export { requestResults as RESULT_SCHEMAS };