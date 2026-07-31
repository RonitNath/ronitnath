import { render } from "solid-js/web";

import EventRsvp from "../solid/EventRsvp";

const mount = document.getElementById("event-rsvp-island");
const endpoint = mount?.dataset.endpoint;
if (mount && endpoint) render(() => <EventRsvp endpoint={endpoint} />, mount);
