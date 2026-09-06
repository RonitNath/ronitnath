/** The grounding caption: where on Earth the viewer is looking out from.
 *
 * It renders empty on the server and stays empty until the city catalog has
 * been fetched and validated, because a coordinate without a place is half a
 * sentence and a place the observer is not over is a lie. */
export function Grounding({ text }: { text: string }) {
  return (
    <p className="grounding" aria-live="off">
      {text}
    </p>
  );
}
