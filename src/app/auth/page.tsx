import { permanentRedirect } from 'next/navigation';

/* The door moved to `/auth/sign-in`. This is where every link written before
 * it moved still lands, and a 308 is what tells a browser, a bookmark and a
 * crawler that it moved for good. */
export default function AuthPage(): never {
  permanentRedirect('/auth/sign-in');
}
