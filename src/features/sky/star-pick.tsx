'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

import { StarDetailPanel } from './detail';
import type { PickHit } from './pick';
import type { Stage } from './stage';

/** Hover, click, and the panel they open — the interaction half of S3.
 *
 * Two rules shape all of it. The pick runs *once a frame at most*, off a
 * pointer position kept in a ref, because `pointermove` fires far faster than
 * the sky is drawn and a projection of two catalogues is not a thing to do per
 * event. And the tag follows the *star*, not the cursor: the sky turns under a
 * still pointer, so a tag left where the mouse stopped drifts off the thing it
 * names within a second.
 *
 * React sees a hover only when the star under the pointer changes. The tag's
 * position is written straight to the DOM by the same loop that picks, the way
 * the callouts are moved (`callouts.tsx`).
 */

/** How far the tag sits from the star, in CSS pixels, and how wide it can be
 * before the flip to the other side is needed to keep it on screen. */
const TAG_OFFSET_PX = 18;
const TAG_WIDTH_PX = 240;

/** Chrome a pointer may be over without it being a pick: everything that is
 * already a control, plus the panel itself. */
const CHROME = '.sky-chrome, .topbar, .home-card, .star-detail, .star-callout, a, button';

function overChrome(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(CHROME) !== null;
}

export function StarPick({
  stage,
  pinned,
  onPin,
}: {
  stage: Stage | null;
  /** The star the panel is open on. Owned by `sky-stage.tsx`, because a
   * callout opens the same panel this does. */
  pinned: PickHit | null;
  onPin: (hit: PickHit | null) => void;
}) {
  const [hover, setHover] = useState<PickHit | null>(null);
  const tag = useRef<HTMLDivElement>(null);
  const hovered = useRef<PickHit | null>(null);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const dirty = useRef(false);

  const pin = onPin;
  const unpin = useCallback(() => onPin(null), [onPin]);

  useEffect(() => {
    if (!stage) return;
    let frame = 0;

    const clearHover = (): void => {
      if (!hovered.current) return;
      hovered.current = null;
      stage.setHover(null);
      setHover(null);
    };

    const onMove = (event: PointerEvent): void => {
      // A touch is a tap, not a hover: a coarse pointer has no "before the
      // click" to show a tag in, and one that lingers after a tap is a tag
      // stuck under a finger.
      if (event.pointerType !== 'mouse') return;
      if (overChrome(event.target)) {
        pointer.current = null;
        clearHover();
        return;
      }
      pointer.current = { x: event.clientX, y: event.clientY };
      dirty.current = true;
    };

    const onLeave = (): void => {
      pointer.current = null;
      clearHover();
    };

    const onClick = (event: PointerEvent): void => {
      if (overChrome(event.target)) return;
      const hit = stage.pickAt(event.clientX, event.clientY);
      pin(hit);
      if (event.pointerType !== 'mouse') clearHover();
    };

    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') unpin();
    };

    /* One frame: pick if the pointer has moved, then move the tag onto
     * whatever star is being pointed at, wherever it has turned to since. */
    const tick = (): void => {
      frame = requestAnimationFrame(tick);
      if (dirty.current && pointer.current) {
        dirty.current = false;
        const hit = stage.pickAt(pointer.current.x, pointer.current.y);
        if (hit?.key !== hovered.current?.key) {
          hovered.current = hit;
          stage.setHover(hit);
          setHover(hit);
        }
      }
      const node = tag.current;
      const current = hovered.current;
      if (!node || !current) return;
      const at = stage.screenPosition(current.position);
      if (!at) {
        node.hidden = true;
        return;
      }
      node.hidden = false;
      // Right of the star by default, left of it when the frame ends first —
      // either way beside it, never on it.
      const flip = at[0] + TAG_OFFSET_PX + TAG_WIDTH_PX > innerWidth;
      const x = flip ? at[0] - TAG_OFFSET_PX : at[0] + TAG_OFFSET_PX;
      node.dataset.side = flip ? 'left' : 'right';
      node.style.transform = `translate3d(${x.toFixed(1)}px, ${at[1].toFixed(1)}px, 0)`;
    };

    addEventListener('pointermove', onMove, { passive: true });
    addEventListener('pointerdown', onClick);
    addEventListener('pointerleave', onLeave);
    addEventListener('keydown', onKey);
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      removeEventListener('pointermove', onMove);
      removeEventListener('pointerdown', onClick);
      removeEventListener('pointerleave', onLeave);
      removeEventListener('keydown', onKey);
      stage.setHover(null);
    };
  }, [stage, pin, unpin]);

  return (
    <>
      <div className="star-tag" ref={tag} hidden aria-hidden="true">
        {hover ? (
          <>
            <strong>{hover.name ?? tagName(hover)}</strong>
            <span>
              mag {hover.magnitude.toFixed(2)} · {hover.colour}
            </span>
          </>
        ) : null}
      </div>
      {pinned ? <StarDetailPanel hit={pinned} onClose={unpin} /> : null}
    </>
  );
}

/** What an unnamed star is called: the catalogue and the number, which is the
 * only name it has. */
export function tagName(hit: PickHit): string {
  const [kind, number] = hit.key.split('-');
  return kind === 'hip' ? `HIP ${number}` : `Gaia DR3 ${number}`;
}
