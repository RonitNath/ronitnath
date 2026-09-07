import type { RefObject } from 'react';

/** The sky itself: one full-viewport canvas the stage paints onto.
 *
 * `.starscape` is the WebGL2 one — the Milky Way's fragment shader and the
 * star points share a context, so the whole sky is one canvas and one clear.
 * `.starscape-flat` is the 2D fallback for a browser with no WebGL2, and the
 * stage hides whichever of the two it did not attach: two skies over each
 * other would be twice the stars at half the brightness.
 *
 * Neither carries state or listeners: the stage owns the clock, the observer
 * and the frame loop, so that a canvas, a globe and a caption cannot disagree
 * about where the viewer is.
 */
export function SkyCanvas({
  canvasRef,
  flatRef,
}: {
  canvasRef: RefObject<HTMLCanvasElement | null>;
  flatRef: RefObject<HTMLCanvasElement | null>;
}) {
  return (
    <>
      <canvas ref={canvasRef} className="starscape" aria-hidden="true" />
      <canvas ref={flatRef} className="starscape-flat" aria-hidden="true" />
    </>
  );
}
