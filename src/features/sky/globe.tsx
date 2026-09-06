import { type RefObject, useEffect, useRef } from 'react';

import type { Stage } from './stage';

/** A drag is a drag and not a click: the pointer has to stay inside a few
 * pixels for the release to be read as a choice of place. */
const CLICK_SLOP_PX = 4;

/** The mini-globe: the same viewpoint the sky is drawn from, seen from
 * outside. Dragging spins the Earth and takes the viewpoint with it; a click
 * travels to the point under it. Both write the one observer the stage owns,
 * so the sky, the marker and the caption cannot disagree. */
export function Globe({
  canvasRef,
  stage,
}: {
  canvasRef: RefObject<HTMLCanvasElement | null>;
  stage: Stage | null;
}) {
  const drag = useRef<{ x: number; y: number; moved: number } | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !stage) return;

    const down = (event: PointerEvent) => {
      canvas.setPointerCapture(event.pointerId);
      drag.current = { x: event.clientX, y: event.clientY, moved: 0 };
    };
    const move = (event: PointerEvent) => {
      const from = drag.current;
      if (!from) return;
      const [dx, dy] = [event.clientX - from.x, event.clientY - from.y];
      drag.current = {
        x: event.clientX,
        y: event.clientY,
        moved: from.moved + Math.hypot(dx, dy),
      };
      stage.drag(dx, dy);
    };
    const up = (event: PointerEvent) => {
      const from = drag.current;
      drag.current = null;
      if (!from) return;
      if (from.moved <= CLICK_SLOP_PX) {
        const point = stage.pointAt(event.clientX, event.clientY);
        if (point) stage.setObserver(point[0], point[1], true);
      }
    };

    canvas.addEventListener('pointerdown', down);
    canvas.addEventListener('pointermove', move);
    canvas.addEventListener('pointerup', up);
    canvas.addEventListener('pointercancel', up);
    return () => {
      canvas.removeEventListener('pointerdown', down);
      canvas.removeEventListener('pointermove', move);
      canvas.removeEventListener('pointerup', up);
      canvas.removeEventListener('pointercancel', up);
    };
  }, [canvasRef, stage]);

  return (
    <div className="mini-globe-frame">
      <canvas
        ref={canvasRef}
        className="mini-globe"
        width={320}
        height={320}
        role="application"
        tabIndex={0}
        aria-label="Choose an observer location on the globe"
      />
    </div>
  );
}
