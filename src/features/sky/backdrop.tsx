import { Atmosphere } from './atmosphere';

/**
 * The sky as scenery rather than as the subject.
 *
 * On this site the sky is not a page, it is what the site looks like, and the
 * landing page was only the first place that showed. Every public page a
 * stranger can reach gets the same thing behind it — an invitation, a
 * published document, a URL that does not resolve — so that arriving anywhere
 * is arriving at the same place.
 *
 * It is the atmosphere with its instruments taken off. Behind a page whose
 * content is something else, the mini-globe, the pause control and the
 * callouts naming the bright stars are a second interface arguing with the
 * first, and they are absent rather than hidden: nothing is drawn that could
 * be read, hovered or clicked.
 *
 * Not everywhere, and deliberately. `/auth` says in its own stylesheet that
 * the starscape is the public presence and the door is where the application
 * starts; `/about` is a paper about the sky and says the picture it describes
 * is one click away; `/u` and `/o` are indoors. Those three rulings stand, and
 * a backdrop that ignored them would be this component overruling the pages it
 * is meant to serve.
 */
export function SkyBackdrop() {
  return <Atmosphere chrome={false} />;
}

/**
 * One sheet of glass over the sky, for a page whose content is prose.
 *
 * Ink on the starfield is ink over moving contrast. The sheet is the answer,
 * and it is the same recipe everywhere it appears so that two pages over the
 * same sky are recognisably the same site (`sky-sheet.css`).
 */
export function SkySheet({
  children,
  measure,
}: {
  children: React.ReactNode;
  /** `narrow` for a page that already has a measure of its own. */
  measure?: 'narrow';
}) {
  return <div className={measure ? `sky-sheet ${measure}` : 'sky-sheet'}>{children}</div>;
}
