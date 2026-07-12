import { render } from "solid-js/web";
import EventRsvp from "../islands/EventRsvp";

const mount = document.getElementById("event-rsvp-island");
const token = mount?.dataset.token;
if (mount && token) {
  mount.textContent = "";
  render(() => <EventRsvp token={token} />, mount);
}
