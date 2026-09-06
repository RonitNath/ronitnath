import type { RefObject } from 'react';

/** The sky itself: a full-viewport 2D canvas the stage paints the real
 * catalog onto. It carries no state and no listeners — the stage owns the
 * clock, the observer and the frame loop, so that a canvas, a globe and a
 * caption cannot disagree about where the viewer is. */
export function SkyCanvas({ canvasRef }: { canvasRef: RefObject<HTMLCanvasElement | null> }) {
  return <canvas ref={canvasRef} className="starscape" aria-hidden="true" />;
}
