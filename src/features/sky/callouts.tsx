import { type CSSProperties, useEffect, useRef, useState } from 'react';

import {
  type Box,
  chooseLeader,
  chromeBoxes,
  keepOutFor,
  LABEL_PX,
  labelLeft,
  labelWidthFor,
  LEADER_SVG_PX,
  type LeaderName,
  leaderLine,
  leaderOffset,
  type Placement,
  positionAttribute,
  RING_RADIUS_PX,
} from './annotate';
import type { NamedStar } from './catalog';
import type { PickHit } from './pick';
import type { Vec3 } from './sidereal';
import { starDetail } from './label';
import type { Stage } from './stage';

/** Named-star callouts, drawn over the sky.
 *
 * They name what is overhead right now — the catalog is IAU names with SIMBAD
 * classifications and distances, so every line is something the sky actually
 * contains, which is why a callout carries the constellation and the distance
 * rather than a caption about the sky. Each one is a button that points at its
 * star: clicking it rings that star in the canvas.
 *
 * The split that matters here is between *which* stars are labelled and
 * *where* their labels are. The first is a React render — it happens when the
 * set changes, which is every half-minute or so. The second happens sixty
 * times a second, and it never touches React: the frame loop writes one
 * transform per callout straight onto the DOM. Pushing positions through state
 * and percentage `left`/`top` is what made the labels step across the sky
 * instead of gliding, because every update re-rendered and re-laid-out them.
 *
 * A callout is offset from its star with a line back to a ring around it, so
 * the name never sits on the thing it names. The whole group — ring, line and
 * label — is one element moved by one transform, so they cannot separate.
 */

/** What React renders. Everything in here is constant for as long as the star
 * is labelled; the numbers that change live in the frame loop. */
interface Rendered {
  star: number;
  vector: Vec3;
  position: string;
  forced: boolean;
}

/** The identity of a *set* of callouts. A change here, and only a change here,
 * is a re-render. */
export function calloutSetKey(placements: readonly Placement[]): string {
  return placements
    .map((placement) => `${placement.star}${placement.forced ? '!' : ''}`)
    .join(' ');
}

/** Everything one frame writes for one callout. Separated from the DOM so the
 * split between per-frame and per-set work can be asserted about. */
export interface CalloutFrame {
  x: number;
  y: number;
  leader: LeaderName;
}

export function calloutFrame(
  placement: Placement,
  width: number,
  height: number,
  label: readonly [number, number] = [labelWidthFor(width), LABEL_PX[1]],
  blocks?: readonly Box[],
): CalloutFrame {
  const offset = leaderOffset(width);
  return {
    x: ((placement.x + 1) / 2) * width,
    y: ((1 - placement.y) / 2) * height,
    leader: chooseLeader(placement, offset, {
      width,
      height,
      label,
      keepOut: keepOutFor(width, height),
      blocks,
    }),
  };
}

/** The page's own chrome, off the page itself. The header and the corner block
 * are laid out by CSS and change size with the viewport and with the controls
 * in them, so they are measured rather than assumed; where there is nothing to
 * measure the estimate in `annotate.ts` stands in.
 *
 * The detail panel joins them while it is open — it is a pane of glass a third
 * of the frame tall, and a callout placed under it is a label nobody can read
 * (docs/sky-plan.md S3). It is *added* to the estimate rather than replacing
 * it, so an open panel is a keep-out even where the other two could not be
 * measured. */
export function measureChrome(width: number, height: number): Box[] {
  const boxes: Box[] = [];
  for (const selector of ['.topbar', '.sky-chrome', '.star-detail']) {
    const node = document.querySelector(selector);
    if (!node) continue;
    const box = node.getBoundingClientRect();
    if (box.width > 0 && box.height > 0)
      boxes.push({ left: box.left, top: box.top, right: box.right, bottom: box.bottom });
  }
  const panel = boxes.length === 3 ? boxes.slice(2) : [];
  return boxes.length >= 2 ? boxes : [...chromeBoxes(width, height), ...panel];
}

/** A callout's own label box, measured when it mounted. The CSS max-width is
 * a function of the viewport, so a label measured at one width is re-measured
 * at another rather than clamped against a box it no longer has. */
function labelSize(node: HTMLElement, width: number): [number, number] {
  if (Number(node.dataset.labelVw) !== width) measureLabel(node, width);
  return [
    Number(node.dataset.labelWidth) || labelWidthFor(width),
    Number(node.dataset.labelHeight) || LABEL_PX[1],
  ];
}

function measureLabel(node: HTMLElement, width: number): void {
  const label = node.querySelector('.callout-label');
  if (!label) return;
  const box = label.getBoundingClientRect();
  // Rounded *up*: a label measured half a pixel short is a label placed half a
  // pixel over the margin it was clamped to.
  node.dataset.labelWidth = String(Math.ceil(box.width));
  node.dataset.labelHeight = String(Math.ceil(box.height));
  node.dataset.labelVw = String(width);
}

/** Move one callout: one transform, and nothing else in the common frame. The
 * leader's geometry is written only when the direction changes, which is a
 * handful of times over a star's whole crossing of the sky. */
function writeFrame(
  node: HTMLElement,
  frame: CalloutFrame,
  offset: number,
  labelWidth: number,
): void {
  node.style.transform = `translate3d(${frame.x.toFixed(2)}px, ${frame.y.toFixed(2)}px, 0)`;
  // The label's own offset is a function of the direction and of where the
  // frame ends, not of the star's position, so it is written when one of those
  // changes and not once a frame.
  const line = leaderLine(frame.leader, offset);
  const anchor = labelLeft(frame.x + line.dx, frame.leader, labelWidth, innerWidth) - frame.x;
  const state = `${frame.leader}:${anchor.toFixed(0)}`;
  if (node.dataset.leader === state) return;
  node.dataset.leader = state;
  node.style.setProperty('--callout-x', `${anchor.toFixed(2)}px`);
  node.style.setProperty('--lead-y', `${line.dy.toFixed(2)}px`);
  const svgLine = node.querySelector('.callout-line');
  if (svgLine) {
    svgLine.setAttribute('x1', line.x1.toFixed(2));
    svgLine.setAttribute('y1', line.y1.toFixed(2));
    svgLine.setAttribute('x2', line.x2.toFixed(2));
    svgLine.setAttribute('y2', line.y2.toFixed(2));
  }
}

export function Callouts({
  stage,
  named,
  onOpen,
  panelOpen = false,
}: {
  stage: Stage | null;
  named: NamedStar[];
  /** A callout is a button onto the detail panel: the same panel a pick out
   * of the sky opens, on the star the callout names. */
  onOpen?: (hit: PickHit) => void;
  /** Whether the panel is open, so its box joins the keep-outs. */
  panelOpen?: boolean;
}) {
  const [rendered, setRendered] = useState<Rendered[]>([]);
  const nodes = useRef(new Map<number, HTMLElement>());
  const latest = useRef<Placement[]>([]);
  /** The chrome's boxes, measured off the page and kept until something can
   * have changed them. Reading them once a frame would be a layout read in the
   * one loop that must not do any. */
  const chrome = useRef<Box[] | null>(null);

  /** A node is positioned the moment it mounts rather than on the next frame,
   * or the first sixteen milliseconds of its entrance play in the corner. */
  const mount = (star: number, node: HTMLElement | null): void => {
    if (!node) {
      nodes.current.delete(star);
      return;
    }
    nodes.current.set(star, node);
    // Measured when the callout mounts: the frame loop needs the label's real
    // box to keep it on screen and must not read layout to get it.
    measureLabel(node, innerWidth);
    const placement = latest.current.find((candidate) => candidate.star === star);
    if (placement) {
      const label = labelSize(node, innerWidth);
      chrome.current ??= measureChrome(innerWidth, innerHeight);
      writeFrame(
        node,
        calloutFrame(placement, innerWidth, innerHeight, label, chrome.current),
        leaderOffset(innerWidth),
        label[0],
      );
    }
  };

  useEffect(() => {
    if (!stage) return;
    let frame = 0;
    let key = '';
    // The chrome is re-measured when the viewport changes and when a control
    // comes or goes — the block grows a row when "Resume orbit" appears — and
    // never inside the frame loop.
    const remeasure = (): void => {
      chrome.current = measureChrome(innerWidth, innerHeight);
    };
    remeasure();
    addEventListener('resize', remeasure);
    const bar = document.querySelector('.sky-chrome');
    const watcher =
      bar && typeof ResizeObserver !== 'undefined' ? new ResizeObserver(remeasure) : null;
    watcher?.observe(bar!);
    const tick = (): void => {
      frame = requestAnimationFrame(tick);
      const placements = stage.placements();
      latest.current = placements;
      const next = calloutSetKey(placements);
      if (next !== key) {
        key = next;
        setRendered(
          placements.map((placement) => ({
            star: placement.star,
            vector: placement.position,
            position: positionAttribute(placement),
            forced: placement.forced,
          })),
        );
      }
      const [width, height] = [innerWidth, innerHeight];
      const offset = leaderOffset(width);
      const blocks = chrome.current ?? undefined;
      for (const placement of placements) {
        const node = nodes.current.get(placement.star);
        if (!node) continue;
        const label = labelSize(node, width);
        writeFrame(
          node,
          calloutFrame(placement, width, height, label, blocks),
          offset,
          label[0],
        );
      }
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      removeEventListener('resize', remeasure);
      watcher?.disconnect();
    };
  }, [stage]);

  /* The panel is not laid out by the time it is asked for, so its box is taken
   * after the commit that opened it — and given back when it closes. */
  useEffect(() => {
    chrome.current = measureChrome(innerWidth, innerHeight);
  }, [panelOpen]);

  return (
    <div className="star-annotations">
      {rendered.map((callout) => {
        const star = named[callout.star];
        if (!star) return null;
        const detail = starDetail(star);
        return (
          <div
            key={star.name}
            className={callout.forced ? 'star-callout is-forced' : 'star-callout'}
            data-star={callout.star}
            data-name={star.name}
            data-position={callout.position}
            data-forced={callout.forced ? 'true' : undefined}
            style={
              {
                '--ring-color': stage?.namedColor(callout.star) ?? 'currentColor',
              } as CSSProperties
            }
            ref={(node) => mount(callout.star, node)}
          >
            {/* Centred on the star: the viewBox puts user unit (0, 0) at the
                group's origin, so the ring sits on the star and the line runs
                out to the label's anchor, both inside the one transform that
                moves the callout. */}
            <svg
              className="callout-leader"
              width={LEADER_SVG_PX}
              height={LEADER_SVG_PX}
              viewBox={`${-LEADER_SVG_PX / 2} ${-LEADER_SVG_PX / 2} ${LEADER_SVG_PX} ${LEADER_SVG_PX}`}
              aria-hidden="true"
            >
              <line className="callout-line" x1="0" y1="0" x2="0" y2="0" />
              <circle className="callout-ring" cx="0" cy="0" r={RING_RADIUS_PX} />
            </svg>
            <button
              type="button"
              className="callout-label"
              aria-label={`${star.name}, ${detail}`}
              onClick={() => {
                const hit = stage?.brightHit(star.brightIndex) ?? null;
                if (hit && onOpen) onOpen(hit);
                else stage?.ringStar(callout.vector);
              }}
            >
              <strong>{star.name}</strong>
              <span>{detail}</span>
            </button>
          </div>
        );
      })}
    </div>
  );
}
