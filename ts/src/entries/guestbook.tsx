import { render } from "solid-js/web";
import Guestbook from "../islands/Guestbook";

const mount = document.getElementById("guestbook-island");
if (mount) {
  mount.textContent = "";
  render(() => <Guestbook />, mount);
}
