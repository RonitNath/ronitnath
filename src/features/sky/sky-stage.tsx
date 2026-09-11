'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

import { Callouts } from './callouts';
import { Globe } from './globe';
import { Grounding } from './grounding';
import type { PickHit } from './pick';
import { SkyCanvas } from './sky-canvas';
import { StarPick } from './star-pick';
import { type Readout, Stage } from './stage';

const EMPTY: Readout = {
  grounding: '',
  named: [],
  manual: false,
  paused: false,
  lines: false,
};

/** Where the constellation toggle is remembered. Off is the default and the
 * absent value, so a visitor who has never pressed it gets the sky. */
const LINES_KEY = 'rn.sky.lines';

/** Storage is a permission in some browsers and absent in others; a sky that
 * throws on load because it could not read a preference is a worse sky than
 * one that forgets. */
function storedLines(): boolean {
  try {
    return localStorage.getItem(LINES_KEY) === '1';
  } catch {
    return false;
  }
}

function rememberLines(on: boolean): void {
  try {
    localStorage.setItem(LINES_KEY, on ? '1' : '0');
  } catch {
    // Nothing to do: the toggle still works for this visit.
  }
}

/** The one client island on the landing page.
 *
 * It owns a {@link Stage} — the clock, the observer, both canvases and the
 * asset fetches — and re-renders only the HTML parts: the callouts, the
 * caption and the controls. The instant is handed down from the server so SSR
 * and hydration agree about which sky this is; the client re-syncs from its
 * own clock against that offset from then on.
 *
 * With `chrome` false none of those HTML parts exist. The stage still runs — it
 * is what draws the sky — but nothing is rendered that could be read, hovered
 * or clicked, so the constellation figures are held off too: the toggle that
 * would turn them back on is one of the things that is gone. Behind a page
 * whose content is something else, the instruments are a second interface
 * arguing with the first. */
export function SkyStage({
  chrome = true,
  serverEpochMs,
}: {
  chrome?: boolean;
  serverEpochMs: number;
}) {
  const skyRef = useRef<HTMLCanvasElement>(null);
  const flatRef = useRef<HTMLCanvasElement>(null);
  const globeRef = useRef<HTMLCanvasElement>(null);
  const [stage, setStage] = useState<Stage | null>(null);
  const [readout, setReadout] = useState<Readout>(EMPTY);
  /* One star is open at a time, and two things open it: a pick out of the sky
   * and a click on a named-star callout. The state lives here so that both
   * reach the same panel. */
  const [pinned, setPinned] = useState<PickHit | null>(null);

  const pin = useCallback(
    (hit: PickHit | null) => {
      setPinned(hit);
      stage?.setPinned(hit);
    },
    [stage],
  );

  useEffect(() => {
    const created = new Stage(serverEpochMs);
    created.attach(skyRef.current, flatRef.current, globeRef.current);
    created.setLines(chrome ? storedLines() : false);
    const unsubscribe = created.subscribe(setReadout);
    created.start();
    setStage(created);

    const reduced = matchMedia('(prefers-reduced-motion: reduce)');
    const onReduced = () => created.setReducedMotion(reduced.matches);
    const onResize = () => created.redraw();
    // Under reduced motion the sky is a still picture, so a theme flip has to
    // ask for the one frame the loop would otherwise have supplied.
    const theme = new MutationObserver(() => created.redraw());
    theme.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
    reduced.addEventListener('change', onReduced);
    addEventListener('resize', onResize);

    return () => {
      unsubscribe();
      created.dispose();
      theme.disconnect();
      reduced.removeEventListener('change', onReduced);
      removeEventListener('resize', onResize);
    };
  }, [chrome, serverEpochMs]);

  /* Not hidden and not pushed behind: absent, and taking no clicks. */
  if (!chrome) {
    return <SkyCanvas canvasRef={skyRef} flatRef={flatRef} />;
  }

  return (
    <>
      <SkyCanvas canvasRef={skyRef} flatRef={flatRef} />
      <div className="sky-chrome">
        <Globe canvasRef={globeRef} stage={stage} />
        <Grounding text={readout.grounding} />
        <div className="sky-controls">
          <button
            type="button"
            className="sky-control"
            onClick={() => stage?.setPaused(!readout.paused)}
          >
            {readout.paused ? 'Resume sky' : 'Pause sky'}
          </button>
          <button
            type="button"
            className="sky-control"
            id="toggle-lines"
            aria-pressed={readout.lines}
            onClick={() => {
              const next = !readout.lines;
              rememberLines(next);
              stage?.setLines(next);
            }}
          >
            Lines
          </button>
          {readout.manual ? (
            <button
              type="button"
              className="sky-control"
              id="resume-orbit"
              onClick={() => stage?.resumeOrbit()}
            >
              Resume orbit
            </button>
          ) : null}
        </div>
      </div>
      <Callouts stage={stage} named={readout.named} onOpen={pin} panelOpen={pinned !== null} />
      <StarPick stage={stage} pinned={pinned} onPin={pin} />
    </>
  );
}
