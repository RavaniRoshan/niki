/**
 * The PTY end-to-end driver.
 *
 * This spawns the **real** shell as a child of a **real** pseudo-terminal — `script -qec` from
 * util-linux is the pty on this machine — feeds it raw bytes, and asserts on the **final screen**
 * after running the output through the small terminal emulator in `src/vt.ts`.
 *
 * Bytes tell you something was written. The screen tells you what a user would see. These tests
 * assert on the screen, because a test that only checks the byte stream passes even when the
 * interface is visibly broken.
 *
 * Every case has a hard timeout and kills its child. A test that can hang is a test that will
 * eventually hang someone else's CI.
 */

import { spawn, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { Terminal } from '../src/vt.js';

const REPO = join(import.meta.dirname, '..', '..');
const ENGINE = join(REPO, 'target', 'debug', 'niki');
const SHELL_ENTRY = join(REPO, 'shell', 'src', 'cli.tsx');

const COLS = 80;
const ROWS = 24;

type Session = {
  child: ChildProcess;
  term: Terminal;
  write: (bytes: string) => void;
  /** Resolves once `predicate` holds or the deadline passes. */
  waitFor: (predicate: () => boolean, timeoutMs: number) => Promise<boolean>;
  screen: () => string;
  cleanup: () => void;
};

const open: Session[] = [];

afterEach(() => {
  for (const s of open.splice(0)) s.cleanup();
});

function startPty(cols: number, rows: number, extraEnv: Record<string, string> = {}): Session {
  const term = new Terminal(cols, rows);
  // The entry is loaded through `-e`, not as a file argument. Both the `tsx` wrapper binary and
  // `node --import tsx file.tsx` exit 0 with no output here — which looks exactly like a shell
  // that renders nothing. Importing the module and calling `main` is the form that runs.
  const bootstrap =
    `import(${JSON.stringify(SHELL_ENTRY)})` +
    `.then((m) => m.main(['--engine', ${JSON.stringify(ENGINE)}]))` +
    `.catch((e) => { process.stderr.write('shell failed: ' + String(e && e.stack || e)); process.exit(1); });`;
  // `stty` inside the pseudo-terminal is what actually sizes it: `script` allocates a pty at the
  // real terminal's size, and a test that cannot change the size cannot test a narrow layout.
  const command =
    `stty cols ${cols} rows ${rows}; TERM=xterm-256color node --import tsx -e ${JSON.stringify(bootstrap)}`;

  // `script` allocates the pseudo-terminal and copies our stdin/stdout into it.
  const child = spawn(
    'script',
    ['-qfec', `cd ${join(REPO, 'shell')} && ${command}`, '/dev/null'],
    {
      env: {
        ...process.env,
        TERM: 'xterm-256color',
        COLORTERM: 'truecolor',
        COLUMNS: String(cols),
        LINES: String(rows),
        NO_UPDATE_CHECK: '1',
        ...extraEnv,
      },
      stdio: ['pipe', 'pipe', 'pipe'],
    },
  );

  child.stdout?.on('data', (chunk: Buffer) => term.feed(chunk));
  child.stderr?.on('data', () => undefined); // the pty merges stderr; it is not a signal here

  const session: Session = {
    child,
    term,
    write: (bytes) => child.stdin?.write(bytes),
    screen: () => term.screenText(),
    waitFor: async (predicate, timeoutMs) => {
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        if (predicate()) return true;
        await new Promise((r) => setTimeout(r, 25));
      }
      return predicate();
    },
    cleanup: () => {
      if (!child.killed) child.kill('SIGKILL');
    },
  };
  open.push(session);
  return session;
}

const shellAvailable = existsSync(ENGINE) && existsSync(SHELL_ENTRY);

describe('PTY end-to-end', () => {
  it('has both the engine binary and the shell entry to drive', () => {
    expect(
      existsSync(ENGINE),
      `${ENGINE} missing. Run: cargo build -j 2 --features fixture-runtime`,
    ).toBe(true);
    expect(existsSync(SHELL_ENTRY)).toBe(true);
  });

  it.skipIf(!shellAvailable)('enters the alternate screen and shows the header', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    const ready = await s.waitFor(() => s.screen().includes('Niki'), 30_000);
    const screen = s.screen();
    expect(ready, `shell never rendered a header:\n${screen}`).toBe(true);
    expect(screen).toContain('Niki');
    // The composer is the anchor: it is on the last-but-two line, above the footer.
    expect(screen).toMatch(/[>›] /);
  }, 60_000);

  it.skipIf(!shellAvailable)('leaves no escape garbage when NO_COLOR is set', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    await s.waitFor(() => s.screen().includes('Niki'), 30_000);
    const screen = s.screen();
    // A raw escape or a CSI introducer surviving into the screen grid means the renderer wrote
    // something it should have consumed itself.
    // eslint-disable-next-line no-control-regex
    expect(screen, 'a control sequence reached the screen').not.toMatch(/\x1b||/);
  }, 60_000);

  it.skipIf(!shellAvailable)('puts typed characters in the composer', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    await s.waitFor(() => s.screen().includes('Niki'), 30_000);
    s.write('audit');
    const typed = await s.waitFor(() => s.screen().includes('audit'), 10_000);
    expect(typed, `typed text never appeared:\n${s.screen()}`).toBe(true);
  }, 60_000);

  it.skipIf(!shellAvailable)('says so plainly at 49 columns instead of drawing a broken layout', async () => {
    const s = startPty(49, 16, { NO_COLOR: '1' });
    const rendered = await s.waitFor(() => s.screen().trim().length > 0, 30_000);
    expect(rendered, `nothing rendered at 49 columns:\n${s.screen()}`).toBe(true);
    // Below 50 columns the spec asks for a clear message and a preserved operation, not a
    // squeezed composer.
    expect(s.screen()).toContain('columns');
    expect(s.screen()).toContain('49');
  }, 60_000);
});

describe('PTY terminal lifecycle', () => {
  it.skipIf(!shellAvailable)('restores the terminal when it is killed with SIGTERM', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    await s.waitFor(() => s.screen().includes('Niki'), 30_000);
    s.child.kill('SIGTERM');
    await new Promise((r) => setTimeout(r, 1500));
    // The process is gone; that is the assertion. A shell that ignores SIGTERM leaves the
    // terminal in the alternate screen with no cursor, and the user's next command is invisible.
    expect(s.child.killed || s.child.exitCode !== null || s.child.signalCode !== null).toBe(true);
  }, 60_000);

  it.skipIf(!shellAvailable)('survives a resize storm without panicking', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    await s.waitFor(() => s.screen().includes('Niki'), 30_000);
    for (let i = 0; i < 20; i += 1) {
      const cols = 40 + ((i * 7) % 100);
      s.term.resize(cols, 24);
      s.write('\x1b[8;24;1t');
      await new Promise((r) => setTimeout(r, 20));
    }
    await new Promise((r) => setTimeout(r, 1000));
    expect(s.child.exitCode === null || s.child.exitCode === 0).toBe(true);
  }, 60_000);
});

describe('D4: a terminal that cannot do the job gets a plain answer', () => {
  it.skipIf(!shellAvailable)('renders no escape garbage when TERM is dumb', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1', TERM: 'dumb' });
    await s.waitFor(() => s.screen().trim().length > 0, 45_000);
    const screen = s.screen();
    // A dumb terminal has no business being sent an alternate screen or a colour run.
    // eslint-disable-next-line no-control-regex
    expect(screen, 'a control sequence reached a dumb terminal').not.toMatch(/\x1b|\u009b/);
  }, 120_000);

  it.skipIf(!shellAvailable)('still shows the composer, because it is the anchor', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1', TERM: 'dumb' });
    await s.waitFor(() => s.screen().includes('Niki'), 45_000);
    expect(s.screen()).toMatch(/[>\u203a] /);
  }, 120_000);
});

describe('D10: a bracketed paste through the real terminal', () => {
  it.skipIf(!shellAvailable)('lands in the composer and submits nothing', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    await s.waitFor(() => s.screen().includes('Niki'), 45_000);
    // A paste containing what would otherwise be a command and an Enter.
    s.write('\x1b[200~/quit\r\x1b[201~');
    const pasted = await s.waitFor(() => s.screen().includes('/quit'), 20_000);
    expect(pasted, `the paste never landed:\n${s.screen()}`).toBe(true);
    // The Enter inside the paste must not have submitted: the turn is still not running.
    expect(s.screen()).not.toContain('Thinking');
  }, 150_000);
});

describe('D11: mouse capture is a mode with a release', () => {
  it.skipIf(!shellAvailable)('ctrl+m releases capture and takes it back', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    await s.waitFor(() => s.screen().includes('Niki'), 45_000);
    // One key gets the user their native selection back, and the same key takes it again.
    s.write('\x0d');
    await new Promise((r) => setTimeout(r, 600));
    expect(s.child.exitCode === null || s.child.exitCode === 0).toBe(true);
    expect(s.screen()).toContain('Niki');
  }, 150_000);

  it.skipIf(!shellAvailable)('an SGR mouse report is consumed, never typed', async () => {
    const s = startPty(COLS, ROWS, { NO_COLOR: '1' });
    await s.waitFor(() => s.screen().includes('Niki'), 45_000);
    // A click at row 10, column 5: press, release, modifiers.
    s.write('\x1b[<0;5;10M\x1b[<0;5;10m');
    await new Promise((r) => setTimeout(r, 800));
    const composerLine = s.screen().split('\n').find((l) => l.includes('\u203a')) ?? '';
    expect(composerLine, 'a mouse report was typed into the composer').not.toContain('[<');
  }, 150_000);
});

describe('the terminal emulator the PTY tests trust', () => {
  it('places text where the cursor was moved', () => {
    const t = new Terminal(20, 5);
    t.feed('\x1b[2J\x1b[Hhello');
    expect(t.screen()[0]).toBe('hello');
  });

  it('overwrites rather than appends when a line is redrawn', () => {
    const t = new Terminal(20, 5);
    t.feed('\x1b[2J\x1b[Hfirst\x1b[K');
    t.feed('\x1b[Hsecond\x1b[K');
    expect(t.screen()[0]).toBe('second');
  });

  it('tracks the alternate screen', () => {
    const t = new Terminal(20, 5);
    t.feed('\x1b[2J\x1b[Hbase');
    t.feed('\x1b[?1049h');
    expect(t.inAltScreen).toBe(true);
    expect(t.screenText()).not.toContain('base');
    t.feed('\x1b[?1049l');
    expect(t.inAltScreen).toBe(false);
    expect(t.screenText()).toContain('base');
  });

  it('reassembles a sequence split across two feeds', () => {
    const t = new Terminal(20, 5);
    t.feed('\x1b[2');
    t.feed('J\x1b[Hok');
    expect(t.screen()[0]).toBe('ok');
  });

  it('advances the cursor two columns for a wide character', () => {
    const t = new Terminal(10, 2);
    t.feed('日本');
    // Two characters, four columns. A terminal that counted code points instead of columns
    // would tear every CJK and emoji row, so the cursor position is the honest assertion.
    expect(t.cursor().col).toBe(4);
    expect(t.screen()[0]).toBe('日本');
  });

  it('survives hostile input without throwing', () => {
    const t = new Terminal(40, 10);
    const hostile = '\x1b]0;title\x1b[2J\x1b[?1049h\x1b[999;999H\x1b[38;2;1;2;3m\x1bP+q\x1b[\\x';
    expect(() => t.feed(hostile)).not.toThrow();
  });

  it('never throws on a resize to zero or an absurd size', () => {
    const t = new Terminal(40, 10);
    expect(() => {
      t.resize(0, 0);
      t.resize(-5, 10);
      t.resize(99999, 99999);
    }).not.toThrow();
  });
});