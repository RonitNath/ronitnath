import { mountGuestbook } from "../islands/Guestbook";

const mount = document.getElementById("guestbook-island");
if (mount) {
  mountGuestbook(mount);
}
