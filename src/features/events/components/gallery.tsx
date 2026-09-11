import { encodeId } from '@/lib/ids';

import '../gallery.css';
import type { PhotoRow } from '../queries';
import { PhotoVisibility } from './photos';

/** Where one picture's bytes are. The link travels in the query because an
 *  `<img>` sends the page's cookies but not the page's address. */
export function photoSrc(slug: string, photoId: number, token: string): string {
  const path = `/e/${slug}/photos/${encodeId('photo', photoId)}`;
  return token ? `${path}?l=${encodeURIComponent(token)}` : path;
}

/**
 * The pictures, once the evening has started.
 *
 * Opening one is a link, not a lightbox. A lightbox is a second page
 * pretending to be a layer — it needs a script, a focus trap, a history entry
 * and a way out — and all a guest wanted was to see the picture bigger. The
 * full frame opens in the same tab and the back button is the way out.
 *
 * Every box is given the picture's own proportions through `width`/`height`
 * so the grid reserves the right space before anything loads, even though the
 * box it is drawn in is square: the numbers are there to stop the page moving
 * under the reader, not to size it.
 *
 * `host` turns the same grid into the host's: the pictures they have taken
 * down are in it, dimmed, each with the control that puts it back. A guest is
 * never handed one of those rows — the read path leaves them out.
 *
 * The children are the drop zone when there is one, so that the heading, the
 * grid and the way to add to it are one section. An evening with no pictures
 * and nobody who may add one has no section at all.
 */
export function Gallery({
  slug,
  token = '',
  photos,
  host,
  children,
}: {
  slug: string;
  token?: string;
  photos: PhotoRow[];
  host?: { event: string };
  children?: React.ReactNode;
}) {
  if (photos.length === 0 && !children) return null;
  return (
    <section className="gallery" data-host={host !== undefined}>
      <h2>Pictures</h2>
      <ul className="shots">
        {photos.map((photo) => (
          <li className="shot" key={photo.id} data-hidden={photo.hiddenAt !== null}>
            <a href={photoSrc(slug, photo.id, token)}>
              {/* next/image optimises what it is given a loader for; these
                  bytes come from an authorised route of this app's own, and
                  the box is square whatever the picture is. */}
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                alt={photo.addedBy ? `Added by ${photo.addedBy}` : 'A picture from the evening'}
                src={photoSrc(slug, photo.id, token)}
                width={photo.width}
                height={photo.height}
                loading="lazy"
              />
            </a>
            {host ? (
              <PhotoVisibility
                event={host.event}
                photo={encodeId('photo', photo.id)}
                hidden={photo.hiddenAt !== null}
              />
            ) : null}
          </li>
        ))}
      </ul>
      {children}
    </section>
  );
}
