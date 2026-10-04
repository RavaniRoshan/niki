/**
 * The shell's one application state, and the one function that changes it.
 *
 * `reduce` is pure: same state plus same event gives the same state. It performs no I/O, reads no
 * clock and touches no terminal. Everything the interface shows is derived here from a message
 * the engine actually sent, which is what makes "never invent a value" a property of the code
 * rather than a rule people have to remember.
 *
 * Every string that arrives from the engine passes through `sanitize` on the way in, so no widget
 * ever receives untrusted bytes.
 */

import { sanitize, sanitizeSingleLine } from './sanitize.js';
import type {
  ApprovalRequestParams,
  BranchCreatedParams,
  ContextUsageParams,
  FinalParams,
  PlanItem,
  Provenance,
  ServerNotification,
  SessionReadyParams,
  StageRole,
  TurnEndParams,
  VerdictReadyParams,
} from './protocol/generated/index.js';

export type Phase =
  | 'booting'
  | 'idle'
  | 'composing'
  | 'thinking'
  | 'streaming'
  | 'toolRunning'
  | 'awaitingApproval'
  | 'error'
  | 'interrupted'
  | 'done';

export type ToolState = 'queued' | 'running' | 'done' | 'failed';

export type ToolRow = {
  readonly id: string;
  readonly name: string;
  readonly args: string;
  readonly state: ToolState;
  /** The one-line result the engine sent. `undefined` means there is nothing to show, so nothing
   * is shown — not a placeholder. */
  readonly summary?: string;
  readonly fullRef?: string;
  readonly durationMs?: number;
  readonly progressNote?: string;
  readonly diffPath?: string;
};

export type StageState = 'running' | 'done' | 'failed';

export type StageRow = {
  readonly id: string;
  readonly role: StageRole;
  readonly attempt: number;
  readonly state: StageState;
  readonly summary?: string;
  readonly tokensIn?: number;
  readonly tokensOut?: number;
  readonly costUsd?: number;
  readonly latencyMs?: number;
  readonly retryCount?: number;
  readonly artifactRef?: string;
  readonly provenance?: Provenance;
  readonly failure?: string;
  readonly severity?: 'info' | 'warning' | 'error';
  readonly recovery?: string;
  /** Provider-supplied summaries only. Raw private reasoning never reaches this array. */
  readonly reasoning: readonly string[];
};

export type Message = {
  readonly kind: 'user' | 'assistant';
  readonly text: string;
  /** Streaming text arrives in pieces; this is the accumulated, sanitised text. */
  readonly streaming: boolean;
};

export type Activity = {
  /** What is happening now, built only from what the engine told us. */
  readonly text: string;
  /** The engine's own hint about what comes next. `undefined` renders no "next" line. */
  readonly next?: string;
  /** A tool is in flight, which changes the sweep cadence. */
  readonly toolInFlight: boolean;
  readonly startedAtMs: number;
};

export type ApprovalState = {
  readonly request: ApprovalRequestParams;
  /** Always the option the engine named as safest. Never the first, never Approve in manual mode. */
  readonly focusedOptionId: string;
};

export type AppState = {
  readonly phase: Phase;
  readonly session: SessionReadyParams | null;
  readonly messages: readonly Message[];
  readonly tools: readonly ToolRow[];
  readonly stages: readonly StageRow[];
  readonly notices: readonly { text: string; level: 'info' | 'warning' | 'error' }[];
  readonly plan: readonly PlanItem[];
  readonly activity: Activity | null;
  readonly approval: ApprovalState | null;
  readonly branch: BranchCreatedParams | null;
  readonly diffRefs: readonly string[];
  readonly verdict: VerdictReadyParams | null;
  readonly context: ContextUsageParams | null;
  readonly costUsd: number | null;
  readonly turnEnd: TurnEndParams | null;
  readonly composer: string;
  readonly queue: readonly string[];
  readonly interrupted: boolean;
  readonly cols: number;
  readonly rows: number;
  readonly protocolError: string | null;
  readonly slashMenu: SlashMenuState | null;
  readonly overlay: OverlayName | null;
  readonly showDetails: boolean;
  readonly showStages: boolean;
  readonly mouseCapture: boolean;
  readonly exitArmed: boolean;
  readonly scrollOffset: number;
  readonly workspaceMode: boolean;
};

export type OverlayName =
  | 'palette'
  | 'help'
  | 'settings'
  | 'model'
  | 'theme'
  | 'sessions'
  | 'history';

export type SlashMenuState = {
  readonly query: string;
  readonly selected: number;
};

export function initialState(cols = 80, rows = 24): AppState {
  return {
    phase: 'booting',
    session: null,
    messages: [],
    tools: [],
    stages: [],
    notices: [],
    plan: [],
    activity: null,
    approval: null,
    branch: null,
    diffRefs: [],
    verdict: null,
    context: null,
    costUsd: null,
    turnEnd: null,
    composer: '',
    queue: [],
    interrupted: false,
    cols,
    rows,
    protocolError: null,
    slashMenu: null,
    overlay: null,
    showDetails: false,
    showStages: false,
    mouseCapture: true,
    exitArmed: false,
    scrollOffset: 0,
    workspaceMode: false,
  };
}

/** The reducer needs a clock, so the clock is a parameter and never a global read. */
export type ReduceOptions = { nowMs: number };

function patchTool(
  tools: readonly ToolRow[],
  id: string,
  update: (row: ToolRow) => ToolRow,
): ToolRow[] {
  let found = false;
  const next = tools.map((row) => {
    if (row.id !== id) return row;
    found = true;
    return update(row);
  });
  // A result for a tool whose call never arrived is still shown, under the id we were given. It is
  // not dropped and it is not invented: the engine said this tool ran and produced this result.
  if (found) return next;
  return [
    ...next,
    update({
      id,
      name: 'tool',
      args: '',
      state: 'queued',
      progressNote: undefined,
    }),
  ];
}

function patchStage(
  stages: readonly StageRow[],
  id: string,
  update: (row: StageRow) => StageRow,
): StageRow[] {
  let found = false;
  const next = stages.map((row) => {
    if (row.id !== id) return row;
    found = true;
    return update(row);
  });
  if (found) return next;
  return [
    ...next,
    update({
      id,
      role: 'planner',
      attempt: 1,
      state: 'running',
      reasoning: [],
    }),
  ];
}

/** Activity text built from the role the engine named, per "built from REAL runtime state". */
export function activityTextForRole(role: StageRole): string {
  switch (role) {
    case 'planner':
      return 'Planning';
    case 'coder':
      return 'Editing';
    case 'tester':
      return 'Running tests';
    case 'reviewer':
      return 'Reviewing changes';
    case 'synthesizer':
      return 'Synthesising';
    case 'security_auditor':
      return 'Auditing security';
    case 'red':
      return 'Challenging';
    case 'critic':
      return 'Critiquing';
  }
}

/** The single state transition function for engine events. */
export function reduce(state: AppState, event: ServerNotification, opts: ReduceOptions): AppState {
  switch (event.method) {
    case 'session.ready': {
      const p = event.params;
      return {
        ...state,
        session: {
          ...p,
          project_path: sanitizeSingleLine(p.project_path),
          model: sanitizeSingleLine(p.model),
        },
        phase: 'idle',
      };
    }

    case 'turn.started':
      return {
        ...state,
        phase: 'thinking',
        turnEnd: null,
        interrupted: false,
        messages: [
          ...state.messages,
          { kind: 'user', text: sanitize(event.params.prompt), streaming: false },
        ],
        activity: {
          text: 'Thinking',
          next: undefined,
          toolInFlight: false,
          startedAtMs: opts.nowMs,
        },
      };

    case 'turn.delta': {
      const last = state.messages[state.messages.length - 1];
      const text = sanitize(event.params.text);
      const messages =
        last && last.kind === 'assistant'
          ? [
              ...state.messages.slice(0, -1),
              { ...last, text: last.text + text, streaming: true },
            ]
          : [...state.messages, { kind: 'assistant' as const, text, streaming: true }];
      return {
        ...state,
        phase: 'streaming',
        messages,
        activity:
          state.activity ??
          { text: 'Responding', next: undefined, toolInFlight: false, startedAtMs: opts.nowMs },
      };
    }

    case 'turn.end': {
      const last = state.messages[state.messages.length - 1];
      const messages =
        last && last.kind === 'assistant' && last.streaming
          ? [...state.messages.slice(0, -1), { ...last, streaming: false }]
          : state.messages;
      return {
        ...state,
        messages,
        turnEnd: event.params,
        activity: null,
        phase: state.interrupted ? 'interrupted' : 'done',
      };
    }

    case 'stage.start': {
      const p = event.params;
      return {
        ...state,
        stages: patchStage(state.stages, p.stage_id, (row) => ({
          ...row,
          role: p.role,
          attempt: p.attempt,
          state: 'running',
          reasoning: [],
        })),
        phase: state.approval ? state.phase : 'thinking',
        activity: {
          text: activityTextForRole(p.role),
          next: undefined,
          toolInFlight: false,
          startedAtMs: state.activity?.startedAtMs ?? opts.nowMs,
        },
      };
    }

    case 'stage.token': {
      const p = event.params;
      return {
        ...state,
        stages: patchStage(state.stages, p.stage_id, (row) => ({
          ...row,
          reasoning: [...row.reasoning, sanitizeSingleLine(p.text)],
        })),
      };
    }

    case 'stage.done': {
      const p = event.params;
      const cost = state.costUsd === null ? p.cost_usd : state.costUsd + p.cost_usd;
      return {
        ...state,
        stages: patchStage(state.stages, p.stage_id, (row) => ({
          ...row,
          role: p.role,
          state: 'done',
          summary: sanitizeSingleLine(p.summary),
          tokensIn: p.tokens_in,
          tokensOut: p.tokens_out,
          costUsd: p.cost_usd,
          latencyMs: p.latency_ms,
          retryCount: p.retry_count,
          artifactRef: p.artifact_ref ?? undefined,
          provenance: p.provenance,
        })),
        costUsd: cost,
      };
    }

    case 'stage.failed': {
      const p = event.params;
      return {
        ...state,
        stages: patchStage(state.stages, p.stage_id, (row) => ({
          ...row,
          role: p.role,
          state: 'failed',
          failure: sanitizeSingleLine(p.error),
          severity: p.severity,
          recovery: p.recovery ? sanitizeSingleLine(p.recovery) : undefined,
        })),
        phase: state.approval ? state.phase : 'error',
        activity: null,
      };
    }

    case 'tool.call': {
      const p = event.params;
      return {
        ...state,
        tools: patchTool(state.tools, p.tool_id, (row) => ({
          ...row,
          id: p.tool_id,
          name: sanitizeSingleLine(p.name),
          args: sanitizeSingleLine(p.args),
          state: 'running',
        })),
        phase: 'toolRunning',
        activity: {
          text: `Running ${sanitizeSingleLine(p.name)}`,
          next: undefined,
          toolInFlight: true,
          startedAtMs: state.activity?.startedAtMs ?? opts.nowMs,
        },
      };
    }

    case 'tool.progress':
      return {
        ...state,
        tools: patchTool(state.tools, event.params.tool_id, (row) => ({
          ...row,
          progressNote: sanitizeSingleLine(event.params.note),
        })),
      };

    case 'tool.result': {
      const p = event.params;
      const tools = patchTool(state.tools, p.tool_id, (row) => ({
        ...row,
        state: p.ok ? 'done' : 'failed',
        summary: sanitizeSingleLine(p.summary),
        fullRef: p.full_ref ?? undefined,
        durationMs: p.duration_ms,
      }));
      // Several tools can be in flight; the activity line names one of the live ones, never none.
      const stillRunning = tools.find((t) => t.state === 'running');
      const activity = stillRunning
        ? {
            text: `Running ${stillRunning.name}`,
            next: undefined,
            toolInFlight: true,
            startedAtMs: state.activity?.startedAtMs ?? opts.nowMs,
          }
        : state.activity;
      return { ...state, tools, activity };
    }

    case 'tool.diff':
      return {
        ...state,
        tools: patchTool(state.tools, event.params.tool_id, (row) => ({
          ...row,
          diffPath: sanitizeSingleLine(event.params.path),
        })),
      };

    case 'approval.request': {
      const p = event.params;
      return {
        ...state,
        phase: 'awaitingApproval',
        approval: {
          request: {
            ...p,
            tool: sanitizeSingleLine(p.tool),
            command: sanitizeSingleLine(p.command),
          },
          focusedOptionId: p.safest_option_id,
        },
        activity: null,
      };
    }

    case 'plan.update':
      return {
        ...state,
        plan: event.params.items.map((i) => ({ text: sanitizeSingleLine(i.text), done: i.done })),
      };

    case 'notice':
      return {
        ...state,
        notices: [
          ...state.notices,
          { text: sanitizeSingleLine(event.params.text), level: event.params.level },
        ],
      };

    case 'diff.ready':
      return event.params.ref
        ? { ...state, diffRefs: [...state.diffRefs, event.params.ref] }
        : state;

    case 'verdict.ready':
      return { ...state, verdict: event.params };

    case 'branch.created':
      return { ...state, branch: event.params };

    case 'context.usage':
      return { ...state, context: event.params };

    case 'cost.update':
      return { ...state, costUsd: event.params.usd };

    case 'final': {
      const p: FinalParams = event.params;
      return {
        ...state,
        activity: null,
        approval: null,
        phase: p.error ? 'error' : 'done',
      };
    }

    default: {
      // The client validates every frame first, so this arm is unreachable in practice. It exists
      // so that adding a message to the protocol without handling it here fails the type check
      // rather than silently dropping it.
      const never: never = event;
      return { ...state, protocolError: `unhandled event ${JSON.stringify(never)}` };
    }
  }
}

/** Local-only transitions the user causes, beside `reduce` so there is one state machine. */
export type LocalAction =
  | { kind: 'composer.set'; text: string }
  | { kind: 'turn.submit'; prompt: string }
  | { kind: 'turn.interrupt' }
  | { kind: 'approval.decide' }
  | { kind: 'approval.focus'; optionId: string }
  | { kind: 'command.run'; name: string; args: string }
  | { kind: 'resize'; cols: number; rows: number }
  | { kind: 'protocolError'; message: string }
  | { kind: 'dismissError' }
  | { kind: 'exit.arm' }
  | { kind: 'exit.now' }
  | { kind: 'details.toggle' }
  | { kind: 'stages.toggle' }
  | { kind: 'palette.open' }
  | { kind: 'overlay.close' }
  | { kind: 'editor.open' }
  | { kind: 'history.open' }
  | { kind: 'mouse.toggle' }
  | { kind: 'mode.cycle' }
  | { kind: 'slashMenu.close' }
  | { kind: 'slashMenu.open'; query: string }
  | { kind: 'slashMenu.accept' }
  | { kind: 'scroll'; by: ScrollBy };

export type ScrollBy = 'pageUp' | 'pageDown' | 'home' | 'end' | 'lineUp' | 'lineDown';

export function reduceLocal(state: AppState, action: LocalAction, _opts: ReduceOptions): AppState {
  switch (action.kind) {
    case 'composer.set':
      // A slash at the start of the composer opens the popup without blocking typing.
      return {
        ...state,
        composer: action.text,
        slashMenu: action.text.startsWith('/')
          ? { query: action.text.slice(1).split(' ')[0] ?? '', selected: 0 }
          : null,
      };
    case 'turn.submit':
      return {
        ...state,
        composer: '',
        // Typed while a run is active: the message waits, visibly, above the composer.
        queue: state.activity ? [...state.queue, action.prompt] : state.queue,
      };
    case 'turn.interrupt':
      return { ...state, interrupted: true, activity: null, phase: 'interrupted' };
    case 'approval.decide':
      return { ...state, approval: null, phase: state.interrupted ? 'interrupted' : 'thinking' };
    case 'approval.focus':
      return state.approval
        ? { ...state, approval: { ...state.approval, focusedOptionId: action.optionId } }
        : state;
    case 'command.run':
      return { ...state, composer: '', slashMenu: null };
    case 'resize':
      return { ...state, cols: action.cols, rows: action.rows };
    case 'protocolError':
      return { ...state, protocolError: action.message };
    case 'dismissError':
      return { ...state, protocolError: null, phase: 'idle' };
    case 'exit.arm':
      return { ...state, exitArmed: true };
    case 'exit.now':
      return state;
    case 'details.toggle':
      return { ...state, showDetails: !state.showDetails };
    case 'stages.toggle':
      return { ...state, showStages: !state.showStages };
    case 'palette.open':
      return { ...state, overlay: 'palette' };
    case 'overlay.close':
      return { ...state, overlay: null };
    case 'editor.open':
    case 'history.open':
      return state;
    case 'mouse.toggle':
      return { ...state, mouseCapture: !state.mouseCapture };
    case 'mode.cycle':
      return { ...state, workspaceMode: !state.workspaceMode };
    case 'slashMenu.close':
      return { ...state, slashMenu: null };
    case 'slashMenu.open':
      return { ...state, slashMenu: { query: action.query, selected: 0 } };
    case 'slashMenu.accept':
      return { ...state, slashMenu: null };
    case 'scroll':
      return { ...state, scrollOffset: applyScroll(state.scrollOffset, action.by) };
  }
}

function applyScroll(current: number, by: ScrollBy): number {
  switch (by) {
    case 'home':
      return 0;
    case 'end':
      return 0;
    case 'lineUp':
      return current + 1;
    case 'lineDown':
      return Math.max(0, current - 1);
    case 'pageUp':
      return current + 10;
    case 'pageDown':
      return Math.max(0, current - 10);
  }
}
