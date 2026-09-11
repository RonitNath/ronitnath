import './atmosphere.css';
import './sky-chrome.css';
import './detail.css';
import './sky-sheet.css';
import { SkyStage } from './sky-stage';

/** The sky, back to front: the designed CSS starfield (the picture before the
 * island hydrates, and the picture a browser with no canvas keeps — it fades
 * out for good once the real catalog is drawn, or it would be a second, static
 * sky behind a turning one), the nebula — which in the light theme is the dusk
 * gradient the page *is* — and the island holding the real turning sky, the
 * Milky Way, the mini-globe and the annotations.
 *
 * The instant is stamped here, on the server, so every browser draws the same
 * sky however wrong its own clock is, and so the server's markup and the
 * client's first render agree. Anything laid over this needs its own stacking
 * context.
 *
 * `chrome` is what the sky is *for* on this page. On the landing page the sky
 * is the content, so it carries its instruments: the mini-globe, the grounding
 * caption, the pause and constellation controls, the callouts naming the
 * bright stars, and the pick that opens a star. Behind a page whose content is
 * something else, all of that is a second interface arguing with the first, so
 * none of it is rendered. What is left is what a backdrop is: stars, the Milky
 * Way, and the turn of the sky. */
export function Atmosphere({ chrome = true }: { chrome?: boolean } = {}) {
  return (
    <>
      <div className="starfield" aria-hidden="true">
        <div className="stars-dim" />
        <div className="stars-med" />
        <div className="stars-bright" />
      </div>
      <div className="nebula" aria-hidden="true" />
      <SkyStage chrome={chrome} serverEpochMs={Date.now()} />
    </>
  );
}
