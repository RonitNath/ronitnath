import './atmosphere.css';
import { SkyCanvas } from './sky-canvas';

/** The sky, back to front: the designed CSS starfield (the picture before the
 * island hydrates, and the whole picture under reduced motion), the nebula —
 * which in the light theme is the dusk gradient the page *is* — and the canvas
 * holding the real turning sky. Server-rendered except the canvas island.
 * Anything laid over this needs its own stacking context. */
export function Atmosphere() {
  return (
    <>
      <div className="starfield" aria-hidden="true">
        <div className="stars-dim" />
        <div className="stars-med" />
        <div className="stars-bright" />
      </div>
      <div className="nebula" aria-hidden="true" />
      <SkyCanvas />
    </>
  );
}
