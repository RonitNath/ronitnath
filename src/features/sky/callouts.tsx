import type { CSSProperties } from 'react';

import { cssPosition, type Placement, positionAttribute } from './annotate';
import type { NamedStar } from './catalog';
import { starDetail } from './label';

/** Named-star callouts, drawn over the sky.
 *
 * They name what is overhead right now — the catalog is IAU names with SIMBAD
 * classifications and distances, so every line is something the sky actually
 * contains, which is why a callout carries the constellation and the distance
 * rather than a caption about the sky. Each one is a button that points at its
 * star: clicking it rings that star in the canvas. */
export function Callouts({
  placements,
  named,
  onSelect,
}: {
  placements: Placement[];
  named: NamedStar[];
  onSelect: (placement: Placement) => void;
}) {
  return (
    <div className="star-annotations">
      {placements.map((placement) => {
        const star = named[placement.star];
        if (!star) return null;
        const [left, top] = cssPosition(placement);
        const detail = starDetail(star);
        // A callout is anchored to its star, but *where* on the label that
        // anchor sits slides with the star's position across the frame: dead
        // centre in the middle, by the left edge at the left margin, by the
        // right edge at the right. Centring everything would hang half the
        // text off the screen at either side, which is where a phone puts it.
        const anchor = `${(-50 - placement.x * 50).toFixed(1)}%`;
        return (
          <button
            key={star.name}
            type="button"
            className={placement.forced ? 'star-callout is-forced' : 'star-callout'}
            data-star={placement.star}
            data-name={star.name}
            data-position={positionAttribute(placement)}
            data-forced={placement.forced ? 'true' : undefined}
            aria-label={`${star.name}, ${detail}`}
            style={
              {
                left: `${left.toFixed(2)}%`,
                top: `${top.toFixed(2)}%`,
                '--callout-x': anchor,
              } as CSSProperties
            }
            onClick={() => onSelect(placement)}
          >
            <strong>{star.name}</strong>
            <span>{detail}</span>
          </button>
        );
      })}
    </div>
  );
}
