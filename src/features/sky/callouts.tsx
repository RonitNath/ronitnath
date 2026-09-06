import { type CSSProperties, useEffect, useRef, useState } from 'react';

import {
  chooseLeader,
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
): CalloutFrame {
  const offset = leaderOffset(width);
  return {
    x: ((placement.x + 1) / 2) * width,
    y: ((1 - placement.y) / 2) * height,
    leader: chooseLeader(placement, offset, {
      width,
      height,
      label: [labelWidthFor(width), LABEL_PX[1]],
      keepOut: keepOutFor(width, height),
    }),
  };
}

/** Move one callout: one transform, and nothing else in the common frame. The
 * leader's geometry is written only when the direction changes, which is a
 * handful of times over a star's whole crossing of the sky. */
function writeFrame(node: HTMLElement, frame: CalloutFrame, offset: number): void {
  node.style.transform = `translate3d(${frame.x.toFixed(2)}px, ${frame.y.toFixed(2)}px, 0)`;
  // The label's own offset is a function of the direction and of where the
  // frame ends, not of the star's position, so it is written when one of those
  // changes and not once a frame.
  const line = leaderLine(frame.leader, offset);
  const width = node.dataset.labelWidth ? Number(node.dataset.labelWidth) : LABEL_PX[0];
  const anchor = labelLeft(frame.x + line.dx, frame.leader, width, innerWidth) - frame.x;
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

export function Callouts({ stage, named }: { stage: Stage | null; named: NamedStar[] }) {
  const [rendered, setRendered] = useState<Rendered[]>([]);
  const nodes = useRef(new Map<number, HTMLElement>());
  const latest = useRef<Placement[]>([]);

  /** A node is positioned the moment it mounts rather than on the next frame,
   * or the first sixteen milliseconds of its entrance play in the corner. */
  const mount = (star: number, node: HTMLElement | null): void => {
    if (!node) {
      nodes.current.delete(star);
      return;
    }
    nodes.current.set(star, node);
    // Measured once, when the callout mounts: the frame loop needs the label's
    // real width to keep it on screen and must not read layout to get it.
    const label = node.querySelector('.callout-label');
    if (label)
      node.dataset.labelWidth = String(Math.round(label.getBoundingClientRect().width));
    const placement = latest.current.find((candidate) => candidate.star === star);
    if (placement) {
      writeFrame(
        node,
        calloutFrame(placement, innerWidth, innerHeight),
        leaderOffset(innerWidth),
      );
    }
  };

  useEffect(() => {
    if (!stage) return;
    let frame = 0;
    let key = '';
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
      for (const placement of placements) {
        const node = nodes.current.get(placement.star);
        if (node) writeFrame(node, calloutFrame(placement, width, height), offset);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [stage]);

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
              onClick={() => stage?.ringStar(callout.vector)}
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
