'use client';

import { useEffect, useRef, useState } from 'react';

import { Callouts } from './callouts';
import { Globe } from './globe';
import { Grounding } from './grounding';
import { SkyCanvas } from './sky-canvas';
import { type Readout, Stage } from './stage';

const EMPTY: Readout = {
  grounding: '',
  placements: [],
  named: [],
  manual: false,
  paused: false,
};

/** The one client island on the landing page.
 *
 * It owns a {@link Stage} — the clock, the observer, both canvases and the
 * asset fetches — and re-renders only the HTML parts: the callouts, the
 * caption and the controls. The instant is handed down from the server so SSR
 * and hydration agree about which sky this is; the client re-syncs from its
 * own clock against that offset from then on. */
export function SkyStage({ serverEpochMs }: { serverEpochMs: number }) {
  const skyRef = useRef<HTMLCanvasElement>(null);
  const globeRef = useRef<HTMLCanvasElement>(null);
  const [stage, setStage] = useState<Stage | null>(null);
  const [readout, setReadout] = useState<Readout>(EMPTY);

  useEffect(() => {
    const created = new Stage(serverEpochMs);
    created.attachSky(skyRef.current);
    created.attachGlobe(globeRef.current);
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
  }, [serverEpochMs]);

  return (
    <>
      <SkyCanvas canvasRef={skyRef} />
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
      <Callouts
        placements={readout.placements}
        named={readout.named}
        onSelect={(placement) => stage?.ringStar(placement.position)}
      />
    </>
  );
}
