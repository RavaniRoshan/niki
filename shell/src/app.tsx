/**
 * The shell root: a pure view.
 *
 * `App` takes an `AppState` and a terminal size and renders. It performs no I/O, reads no clock,
 * owns no event loop and calls no engine API — that is the "render is a pure function of AppState
 * and terminal size" mandate taken literally, and it is what makes every frame snapshot-testable
 * without a process, a socket or a terminal.
 *
 * The event loop lives in `cli.tsx`; key matching lives in `dispatch.ts`; state changes happen in
 * `state.ts`. Three modules, three jobs, one of each.
 */

import React from 'react';
import { Box, Text } from 'ink';
import { Masthead } from './components/masthead.js';
import { Transcript } from './components/transcript.js';
import { Composer } from './components/composer.js';
import { Footer } from './components/footer.js';
import { Approval } from './components/approval.js';
import { SlashMenu } from './components/slash-menu.js';
import { Palette } from './components/palette.js';
import { Picker } from './components/picker.js';
import { Help } from './components/help.js';
import { SettingsSheet } from './components/settings-sheet.js';
import { Confirm } from './components/confirm.js';
import type { AppState } from './state.js';
import { paletteFor, type ThemeName } from './theme/index.js';
import type { Charset } from './glyphs.js';

export type AppProps = {
  readonly state: AppState;
  /** The theme the shell booted with. `state.activeTheme` wins once a picker has previewed one. */
  readonly theme: ThemeName;
  readonly charset: Charset;
  readonly reducedMotion: boolean;
  /** Sweep frame index. Static when motion is reduced, so snapshots are stable. */
  readonly sweepTick?: number;
  readonly version?: string;
};

export function App(props: AppProps): React.ReactElement {
  const { state, charset, reducedMotion } = props;
  // The preview lives in the state so rendering stays a pure function of it: the theme picker
  // previews by writing `activeTheme`, and cancelling writes it back.
  const theme = state.activeTheme ?? props.theme;
  const c = paletteFor(theme);
  const sweepTick = reducedMotion ? 0 : (props.sweepTick ?? 0);

  // Responsive tiers from the spec. Under 50 columns the interface says so plainly instead of
  // pretending to be usable, and the operation in flight is named rather than thrown away.
  if (state.cols < 50) {
    return (
      <Box flexDirection="column" width={state.cols}>
        <Text color={c.warning} bold>
          terminal is {state.cols} columns wide
        </Text>
        <Text color={c.muted}>NIKI needs 50 or more. Enlarge the window.</Text>
        {state.activity ? (
          <Text color={c.accent}>
            {state.activity.text} · still running
          </Text>
        ) : null}
      </Box>
    );
  }

  const headerHeight = state.cols >= 80 ? 4 : 1;
  // An overlay replaces the conversation rather than stacking on it: two lists of rows competing
  // for the same twenty-four lines is worse than either one alone.
  const overlayOpen = state.overlay !== null || state.confirm !== null;
  // Masthead, composer (three rows) and footer, plus the approval prompt when one is open.
  const chrome = headerHeight + 1 + 3 + (state.approval ? 8 : 0);
  const transcriptHeight = Math.max(1, state.rows - chrome);
  // What an overlay may spend once the fixed chrome is subtracted: the masthead (three rows of art
  // at 80 columns, two where the one-line tier wraps at a narrower width), the composer's three
  // rows and the footer. Everything below is bounded by this, which is why an overlay can never
  // push the composer off the bottom of the screen.
  const overlayHeight = Math.max(
    4,
    state.rows - (state.cols >= 80 ? 3 : 2) - 4 - (state.approval ? 8 : 0),
  );

  return (
    // `height` plus a growing transcript is what makes the composer an anchor rather than just
    // another row: the transcript absorbs the slack, so the composer and footer stay pinned to
    // the bottom of the screen however long the conversation gets.
    <Box flexDirection="column" width={state.cols}>
      <Masthead state={state} theme={theme} charset={charset} version={props.version} />
      <Box flexDirection="column" flexGrow={1}>
        {state.confirm ? (
          // A safety-critical change is modal: it takes the whole content area, because the one
          // thing that must not happen is the change being made next to something else.
          <Confirm state={state} theme={theme} charset={charset} />
        ) : overlayOpen ? (
          <Box flexDirection="column">
            <Palette state={state} theme={theme} charset={charset} height={overlayHeight} />
            <Picker state={state} theme={theme} charset={charset} height={overlayHeight} />
            <Help state={state} theme={theme} charset={charset} height={overlayHeight} />
            <SettingsSheet state={state} theme={theme} charset={charset} height={overlayHeight} />
          </Box>
        ) : (
          <Transcript
            state={state}
            theme={theme}
            charset={charset}
            reducedMotion={reducedMotion}
            sweepTick={sweepTick}
            height={transcriptHeight}
          />
        )}
      </Box>
      <SlashMenu state={state} theme={theme} charset={charset} />
      <Approval state={state} theme={theme} charset={charset} />
      <Composer state={state} theme={theme} charset={charset} focused={!state.approval} />
      <Footer state={state} theme={theme} charset={charset} overlayOpen={overlayOpen} />
    </Box>
  );
}