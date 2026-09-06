import './atmosphere.css';
import './sky-chrome.css';
import { SkyStage } from './sky-stage';

/** The sky, back to front: the designed CSS starfield (the picture before the
 * island hydrates, and the whole picture under reduced motion), the nebula —
 * which in the light theme is the dusk gradient the page *is* — and the island
 * holding the real turning sky, the mini-globe and the annotations.
 *
 * The instant is stamped here, on the server, so every browser draws the same
 * sky however wrong its own clock is, and so the server's markup and the
 * client's first render agree. Anything laid over this needs its own stacking
 * context. */
export function Atmosphere() {
  return (
    <>
      <div className="starfield" aria-hidden="true">
        <div className="stars-dim" />
        <div className="stars-med" />
        <div className="stars-bright" />
      </div>
      <div className="nebula" aria-hidden="true" />
      <SkyStage serverEpochMs={Date.now()} />
    </>
  );
}
