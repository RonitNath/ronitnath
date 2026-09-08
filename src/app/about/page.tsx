import type { Metadata } from 'next';
import Link from 'next/link';

import { ThemeToggle } from '../theme-toggle';

import './about.css';

export const metadata: Metadata = {
  title: 'About the sky',
  description: 'What the sky on ronitnath.com is, where its data comes from, and how it is drawn.',
};

/* The sky, explained once. No sky here: the page reads like a paper, and the
 * picture it describes is one click away. Every number on it is the number the
 * code uses (src/features/sky), and every source is the one the assets were
 * built from (public/stars/NOTICE, public/textures/earth/NOTICE). */
export default function AboutPage() {
  return (
    <>
      <header className="topbar">
        <Link href="/">Back to the sky</Link>
        <ThemeToggle />
      </header>
      <main className="paper">
        <h1>About the sky</h1>
        <p className="lede">
          The landing page is the real night sky, seen from a real place on Earth at a real
          instant, drawn from published star catalogues. Nothing in it is painted or
          procedural. This page says what you are looking at, where the data came from, and
          what the browser does to it every frame.
        </p>

        <section>
          <h2>What you are looking at</h2>
          <p>
            The camera lies on its back and looks straight up at the zenith, with north at
            the top of the screen. East is on the left, as it is when you look up rather
            than down at a map. The projection is gnomonic: straight lines on the sky stay
            straight on the screen, and the field widens toward the edges the way a
            wide-angle lens does.
          </p>
          <p>
            The point of view moves. It follows a great circle inclined 63° to the equator
            that passes through San Francisco, completing one lap every half sidereal day,
            so over a few minutes the sky drifts as it would for someone circling the
            planet. Time runs at sixty times wall-clock speed. Every visitor sees the same
            sky, because the server stamps the instant into the page and the clock counts
            from a fixed epoch rather than from your machine.
          </p>
          <p>
            Dragging the globe moves the viewpoint anywhere on Earth for your tab only. The
            globe is lit from the direction of the Sun at the simulated instant, so the
            terminator you see is the real one for that time. The caption under it names
            the nearest city with more than fifteen thousand people.
          </p>
          <table>
            <caption>The view</caption>
            <tbody>
              <tr>
                <th scope="row">Field, top edge to bottom edge</th>
                <td>100°</td>
              </tr>
              <tr>
                <th scope="row">Clock rate</th>
                <td>60× real time</td>
              </tr>
              <tr>
                <th scope="row">One full turn of the sky</th>
                <td>23.9 minutes</td>
              </tr>
              <tr>
                <th scope="row">Frame rate</th>
                <td>30 per second</td>
              </tr>
            </tbody>
          </table>
        </section>

        <section>
          <h2>The stars</h2>
          <p>
            The catalogue holds 12,335 stars, every star brighter than magnitude 6.5. That is
            the naked-eye limit from a dark site, so the screen shows what an unaided eye
            could see if the atmosphere and the city were not in the way. Each record is a
            direction on the sky, an apparent magnitude and a display colour.
          </p>
          <p>
            Positions and magnitudes come from Gaia Data Release 3, the European Space
            Agency mission that has measured the positions, distances and colours of
            nearly two billion stars from a spacecraft at the second Lagrange point. Gaia
            saturates on the very brightest stars, so the bright end is taken from the
            Hipparcos catalogue instead, the earlier ESA astrometry mission of 1989 to 1993
            in its 2007 re-reduction: brighter than second magnitude the Hipparcos entry
            wins outright, and the Gaia entry within three arcseconds of it is dropped.
            Below that Gaia is the measurement, and a Hipparcos entry is only added where
            Gaia has nothing within three arcseconds and 1.5 magnitudes. Just under two
            hundred stars are here on those terms, five of them second-magnitude ones the
            older cut passed just above — Sheratan in Aries, Menkar in Cetus, Mahasim in
            Auriga, Gienah in Corvus and Enif in Pegasus. Every Hipparcos position is
            carried forward by the star’s own motion from that catalogue’s epoch of 1991.25
            to Gaia’s of 2016.0 first, or the same star arrives twice.
          </p>
          <p>
            Colour is derived from each star’s BP−RP index, the difference between Gaia’s
            blue and red photometer magnitudes, mapped onto a short ramp from blue-white
            through white to orange. The ramp is tuned to read on a near-black screen; it is
            not a spectrum and does not claim to be.
          </p>
          <p>
            Three hundred and thirty-nine stars are named: every name approved by the
            International Astronomical Union whose star is bright enough to be in the
            catalogue. Constellations, classifications and distances come from the HYG
            catalogue and the Hipparcos parallaxes, with distances rounded for legibility;
            the fifty the site opened with keep the wording they were checked against
            SIMBAD with. At most three are called out at once, chosen for what is in frame.
          </p>
          <p>
            The Lines control draws the constellation figures — 674 segments over 88
            constellations, from Stellarium’s modern sky culture, with each figure’s
            Hipparcos numbers resolved into catalogue records when the file was packed.
            Two of the 676 are missing, both in Canis Major and both drawn to a star
            fainter than the catalogue’s own limit; a segment that cannot be resolved at
            both ends is left out rather than guessed at.
            They are off unless you ask for them, they carry no labels, and they dim with
            the same extinction the stars do.
          </p>
        </section>

        <section>
          <h2>The Milky Way</h2>
          <p>
            The band behind the stars is not an image of the Milky Way. It is a count. Every
            Gaia source brighter than magnitude 15, some 36.8 million stars, was binned into
            3,145,728 equal-area cells of the sky 0.115° across, and each cell’s star count
            became its surface brightness. Where the galactic disc is edge-on
            there are more stars per cell and the band appears on its own. Where
            interstellar dust hides the stars behind it the counts collapse, which is why the
            dark lane through Cygnus and Aquila, the Great Rift, is there without anyone
            drawing it. The band’s warm cast is measured too: flux-weighted colour runs
            redder in the plane than at the galactic poles, which is interstellar reddening.
          </p>
          <p>
            The counts are baked once into a 4096 by 2048 pixel map in equatorial
            coordinates — a 2048 by 1024 version is sent to narrow screens, which have no
            pixels to show the difference on. The first bake used cells half a degree
            across, and at that size the dust lanes toward the galactic centre were blobs of
            the grid rather than shapes of the dust; the cells are now a fifth of that.
            The browser never contacts the Gaia archive.
          </p>
        </section>

        <section>
          <h2>Deeper than the eye</h2>
          <p>
            The naked-eye catalogue is what arrives first, and it is not where the sky
            stops. Once the page has painted, a second file of every Gaia star brighter
            than magnitude 9 — 165,393 of them — is fetched whole, and after that the sky
            is streamed: 768 tiles of every star brighter than magnitude 12, three million
            in all, requested nearest the middle of the view first and then along the path
            the viewpoint will take over the next few simulated minutes, so the sky ahead
            has arrived by the time you are under it. The faint tiles are drawn as flux
            below the size of a pixel rather than as points, which is what makes them read
            as the texture of the sky instead of as three million dots. Nothing is fetched
            on a metered connection or a device that says it is saving data.
          </p>
          <p>
            Clicking a star opens what is known about it. That comes from this server and
            no other: a 3.09 million row table built offline from Gaia DR3’s source and
            astrophysical parameters, the HYG catalogue for proper names, Bayer and
            Flamsteed designations and spectral types, the IAU name list, the IAU
            constellation boundaries, and one bulk SIMBAD query for the twelve thousand
            stars the pointer can reach. The browser never asks ESA or CDS anything, and
            neither does the page behind it.
          </p>
        </section>

        <section>
          <h2>From catalogue to screen</h2>
          <p>
            Catalogue directions are given in the International Celestial Reference System,
            fixed to the J2000 epoch. Putting one on the screen for a given observer and
            instant takes three rotations, applied in this order.
          </p>
          <ol>
            <li>
              <strong>Precession.</strong> Earth’s axis wobbles with a period of about
              26,000 years, so the equator and equinox of today are not those of the year
              2000. The catalogue frame is rotated to the mean equator and equinox of the
              simulated date.
            </li>
            <li>
              <strong>Sidereal time.</strong> Greenwich mean sidereal time is computed for
              the instant from a standard low-precision series, and the observer’s longitude
              is added to give local sidereal time. This is the angle the sky has turned
              through, and it is the only place Earth’s rotation enters.
            </li>
            <li>
              <strong>Horizon.</strong> Local sidereal time and latitude define the observer’s
              frame: right, north and up. Directions with a zenith cosine below 0.08 are
              near or below the horizon and are dropped.
            </li>
          </ol>
          <p>
            What survives is projected with a focal length of 0.8391, which places the
            horizon-ward edge of the frame 50° from the zenith. Each star’s brightness is
            recovered from its magnitude as 10<sup>−0.4 m</sup>, and that number drives two
            responses carried unchanged from the original site: a point size between one and
            nine pixels, and an opacity between 0.38 and 1. Atmospheric extinction then dims
            each star by 0.28 magnitudes per airmass, the value for a humid low-altitude
            site, so stars fade toward the edge of the frame the way they fade toward a real
            horizon.
          </p>
          <p>
            Every star is drawn as a small additive sprite whose brightness falls off from
            the centre as e<sup>−2.5 d²</sup>. Additive blending is what makes the core of a
            bright star saturate toward white while its colour survives in the halo, which is
            how a bright star looks to the eye. The Milky Way is drawn by a fragment shader
            that runs the three rotations backwards for every pixel to find which direction
            on the sky it shows, samples the count map there, and applies the same
            extinction.
          </p>
        </section>

        <section>
          <h2>Where to go for more</h2>
          <ul className="sources">
            <li>
              <a href="https://gea.esac.esa.int/archive/" rel="noopener" target="_blank">
                ESA Gaia Archive
              </a>{' '}
              — the catalogue the stars and the Milky Way were built from. Gaia DR3 is
              published under CC BY-SA 3.0 IGO.
            </li>
            <li>
              <a
                href="https://gea.esac.esa.int/archive/documentation/GDR3/"
                rel="noopener"
                target="_blank"
              >
                Gaia DR3 documentation
              </a>{' '}
              — what the G, BP and RP magnitudes measure and how the positions were derived.
            </li>
            <li>
              <a
                href="https://cdsarc.cds.unistra.fr/viz-bin/cat/I/311"
                rel="noopener"
                target="_blank"
              >
                Hipparcos, the New Reduction
              </a>{' '}
              — VizieR catalogue I/311, used for the brightest
              stars and the named-star distances.
            </li>
            <li>
              <a
                href="https://www.iau.org/public/themes/naming_stars/"
                rel="noopener"
                target="_blank"
              >
                IAU Catalog of Star Names
              </a>{' '}
              — the proper names the callouts use.
            </li>
            <li>
              <a href="https://simbad.cds.unistra.fr/simbad/" rel="noopener" target="_blank">
                SIMBAD
              </a>{' '}
              — the astronomical database the classifications were checked against.
            </li>
            <li>
              <a href="https://www.geonames.org/" rel="noopener" target="_blank">
                GeoNames
              </a>{' '}
              — the cities15000 export behind the nearest-city caption, CC BY 4.0.
            </li>
            <li>
              <a
                href="https://www.solarsystemscope.com/textures/"
                rel="noopener"
                target="_blank"
              >
                Solar System Scope
              </a>{' '}
              — the globe’s day, normal and specular maps, CC BY 4.0, derived from NASA’s
              Blue Marble imagery.
            </li>
            <li>
              <a href="https://stellarium.org/" rel="noopener" target="_blank">
                Stellarium
              </a>{' '}
              — a full planetarium, for when you want to zoom in, add planets, or check a
              star against this page.
            </li>
            <li>
              <a href="https://github.com/RonitNath/ronitnath" rel="noopener" target="_blank">
                Source code
              </a>{' '}
              — the sky lives in <code>src/features/sky</code>; the catalogue and texture
              provenance notices sit beside the assets.
            </li>
          </ul>
        </section>
      </main>
    </>
  );
}
