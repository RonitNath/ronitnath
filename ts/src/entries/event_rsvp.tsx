import { render } from "solid-js/web";
import EventRsvp from "../islands/EventRsvp";

const mount = document.getElementById("event-rsvp-island");
const endpoint = mount?.dataset.endpoint;
if (mount && endpoint) {
  mount.textContent = "";
  render(() => <EventRsvp endpoint={endpoint} />, mount);
}
