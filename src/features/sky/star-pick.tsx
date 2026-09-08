'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

import type { Box } from './annotate';
import { StarDetailPanel } from './detail';
import type { PickHit } from './pick';
import type { Stage } from './stage';
import { calloutBoxes, chooseTagSide, pageBoxes, TAG_PX, tagBox } from './tag';

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
 *
 * A star the page has already labelled gets no tag. The callout beside it
 * carries the star's name in a bigger hand a few pixels away, and a tag drawn
 * there printed the two over each other; the callout goes into a hover state
 * instead, which is the same answer said once (docs/sky-plan.md S3). Where a
 * tag *is* drawn, `tag.ts` chooses which corner of the star it hangs off, so
 * it lands on no callout, no card and no chrome.
 */

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
  /** The callout currently in the hover state, so it can be given back. */
  const marked = useRef<HTMLElement | null>(null);
  /** The tag's measured box, and the star it was measured for. */
  const measured = useRef<{ key: string; size: [number, number] } | null>(null);
  /** The page's own boxes. Re-measured when the hover or the panel changes and
   * never inside the frame loop, which must read no layout. */
  const page = useRef<Box[] | null>(null);

  const pin = onPin;
  const unpin = useCallback(() => onPin(null), [onPin]);

  useEffect(() => {
    if (!stage) return;
    let frame = 0;

    /** The callout that names this star, where one is on screen. Compared by
     * name because that is what a callout carries: `data-name` is the entry in
     * `named.json` the pick's own `name` came out of. */
    const calloutFor = (hit: PickHit): HTMLElement | null => {
      if (!hit.name) return null;
      for (const node of document.querySelectorAll<HTMLElement>('.star-callout')) {
        if (node.dataset.name === hit.name) return node;
      }
      return null;
    };

    /** At most one callout is in the hover state, and it is this one. Written
     * straight to the DOM: the callouts re-render on a set change, which is
     * once every half-minute, and a hover is not a set change. */
    const markCallout = (node: HTMLElement | null): void => {
      if (marked.current === node) return;
      if (marked.current) delete marked.current.dataset.hover;
      marked.current = node;
      if (node) node.dataset.hover = 'true';
    };

    const clearHover = (): void => {
      markCallout(null);
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
      if (!current) {
        // The pointer left the star. The pick clears itself where it is set,
        // but the callout it lit up is given back here, where every other
        // ending is handled too.
        markCallout(null);
        if (node) node.hidden = true;
        return;
      }
      if (!node) return;
      const at = stage.screenPosition(current.position);
      if (!at) {
        node.hidden = true;
        markCallout(null);
        return;
      }
      // The star already has a name on screen: light that up rather than
      // saying it twice, on top of itself.
      const callout = calloutFor(current);
      markCallout(callout);
      if (callout) {
        node.hidden = true;
        return;
      }
      node.hidden = false;
      // Measured once the tag is showing this star's own text: a box judged on
      // the previous star's width flips at the wrong distance from the edge.
      if (node.dataset.key === current.key && measured.current?.key !== current.key) {
        const box = node.getBoundingClientRect();
        if (box.width > 0) measured.current = { key: current.key, size: [box.width, box.height] };
      }
      const size = measured.current?.key === current.key ? measured.current.size : TAG_PX;
      const where = {
        width: innerWidth,
        height: innerHeight,
        size,
        blocks: [...(page.current ?? []), ...calloutBoxes()] as Box[],
      };
      const side = chooseTagSide(at[0], at[1], where);
      const box = tagBox(at[0], at[1], side, where);
      node.dataset.side = side;
      node.style.transform = `translate3d(${box.left.toFixed(1)}px, ${box.top.toFixed(1)}px, 0)`;
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
      markCallout(null);
      stage.setHover(null);
    };
  }, [stage, pin, unpin]);

  /* The panel is not laid out by the time it opens, and a callout that has
   * just appeared is not either, so the page's boxes are taken after the
   * commit rather than during it. */
  useEffect(() => {
    page.current = pageBoxes();
    const remeasure = (): void => {
      page.current = pageBoxes();
    };
    addEventListener('resize', remeasure);
    return () => removeEventListener('resize', remeasure);
  }, [hover, pinned]);

  return (
    <>
      <div className="star-tag" ref={tag} hidden aria-hidden="true" data-key={hover?.key}>
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
