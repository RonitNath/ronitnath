import type { RefObject } from 'react';

/** The sky itself: two full-viewport canvases the stage paints onto — the
 * Milky Way underneath, drawn by a fragment shader, and the star catalog над
 * it in 2D. Neither carries state or listeners: the stage owns the clock, the
 * observer and the frame loop, so that a canvas, a globe and a caption cannot
 * disagree about where the viewer is. */
export function SkyCanvas({
  bandRef,
  canvasRef,
}: {
  bandRef: RefObject<HTMLCanvasElement | null>;
  canvasRef: RefObject<HTMLCanvasElement | null>;
}) {
  return (
    <>
      <canvas ref={bandRef} className="milkyway" aria-hidden="true" />
      <canvas ref={canvasRef} className="starscape" aria-hidden="true" />
    </>
  );
}
