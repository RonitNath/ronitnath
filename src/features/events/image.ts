/* What a picture is allowed to be, and what has to come out of it first.
 *
 * A photograph is the one thing on the guest page that arrives from outside as
 * bytes rather than as text, and bytes carry more than they look like. A phone
 * writes the coordinates of the room, the make and model of the camera and the
 * second the shutter opened into every JPEG it takes. A guest handing the host
 * a picture of the table is not handing over the address of the party, and a
 * URL anyone with a link can fetch is not the place to find out otherwise.
 *
 * The donor solved this by re-encoding every upload through sharp, which
 * discards metadata because it is never asked to write any. There is no sharp
 * here and adding an image codec to carry a gallery would be a dependency this
 * app does not otherwise need, so the same guarantee is reached the other way:
 * the metadata containers are cut out of the file and the compressed pixels are
 * copied through untouched. Every one of the three accepted formats keeps its
 * metadata in named, length-prefixed blocks that can be walked without decoding
 * an image — a JPEG's APP segments, a PNG's ancillary chunks, a WebP's EXIF and
 * XMP chunks — so removing them is a copy with holes in it, and the picture the
 * guest chose is byte-for-byte the picture that is stored, minus the parts that
 * were never about the picture.
 *
 * The one thing re-encoding gave that this does not is the refusal of a file
 * that claims to be an image and is not: sharp threw on it. {@link sniff}
 * answers the same question from the file's own magic bytes, and the upload
 * path stores what the bytes say rather than what the browser claimed.
 *
 * Everything here is pure and takes no database: it is the part of the feature
 * that can be proved on a table of bytes.
 */

/** The three a browser may send. Each is stored as itself — a JPEG stays a
 *  JPEG, so nobody's picture silently changes kind on the way in. */
export const FORMATS = ['image/jpeg', 'image/png', 'image/webp'] as const;

export type Allowed = (typeof FORMATS)[number];

export const isAllowed = (mime: string): mime is Allowed =>
  (FORMATS as readonly string[]).includes(mime);

/** True for the formats refused by name rather than in general. A phone can
 *  send a JPEG instead, which is a thing the guest can act on, so it is worth
 *  telling them apart from "not a picture". */
export const isHeic = (mime: string): boolean =>
  mime === 'image/heic' || mime === 'image/heif' || mime === 'image/heic-sequence';

export interface Size {
  width: number;
  height: number;
}

/* Indexing a Uint8Array is typed `number | undefined` under this repo's
 * `noUncheckedIndexedAccess`, and every read here is already inside a bounds
 * check the compiler cannot see. One accessor says so once: past the end of a
 * truncated file the answer is zero, which every walk below treats as a header
 * that does not parse. */
function byte(bytes: Uint8Array, at: number): number {
  return bytes[at] ?? 0;
}

function u16be(bytes: Uint8Array, at: number): number {
  return (byte(bytes, at) << 8) | byte(bytes, at + 1);
}

function u16le(bytes: Uint8Array, at: number): number {
  return byte(bytes, at) | (byte(bytes, at + 1) << 8);
}

function u32be(bytes: Uint8Array, at: number): number {
  const low = (byte(bytes, at + 1) << 16) | (byte(bytes, at + 2) << 8) | byte(bytes, at + 3);
  return byte(bytes, at) * 0x1000000 + low;
}

function u32le(bytes: Uint8Array, at: number): number {
  const low = byte(bytes, at) + byte(bytes, at + 1) * 0x100 + byte(bytes, at + 2) * 0x10000;
  return low + byte(bytes, at + 3) * 0x1000000;
}

function ascii(bytes: Uint8Array, at: number, length: number): string {
  let out = '';
  for (let i = 0; i < length; i += 1) out += String.fromCharCode(byte(bytes, at + i));
  return out;
}

const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** What the bytes actually are, or null. The header of each format is
 *  unambiguous in its first twelve bytes; nothing beyond them is trusted. */
export function sniff(bytes: Uint8Array): Allowed | null {
  if (
    bytes.length >= 3 &&
    byte(bytes, 0) === 0xff &&
    byte(bytes, 1) === 0xd8 &&
    byte(bytes, 2) === 0xff
  ) {
    return 'image/jpeg';
  }
  if (bytes.length >= 8 && PNG_MAGIC.every((want, index) => byte(bytes, index) === want)) {
    return 'image/png';
  }
  if (bytes.length >= 12 && ascii(bytes, 0, 4) === 'RIFF' && ascii(bytes, 8, 4) === 'WEBP') {
    return 'image/webp';
  }
  return null;
}

interface Segment {
  marker: number;
  /** Where the 0xFF that opens the segment is. */
  at: number;
  /** The whole segment including its marker and length. */
  length: number;
}

/* A JPEG is a marker stream: 0xFF, a marker byte, and for most markers a
 * big-endian length that includes its own two bytes. The walk stops at the
 * start of the scan, because from there on the file is entropy-coded data in
 * which 0xFF means something else entirely. */
function jpegSegments(bytes: Uint8Array): { segments: Segment[]; scanAt: number } {
  const segments: Segment[] = [];
  let at = 2;
  while (at + 3 < bytes.length) {
    if (byte(bytes, at) !== 0xff) break;
    let marker = byte(bytes, at + 1);
    /* Fill bytes: any number of 0xFF may pad the space before a marker. */
    while (marker === 0xff && at + 2 < bytes.length) {
      at += 1;
      marker = byte(bytes, at + 1);
    }
    /* Standalone markers carry no payload. */
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) {
      at += 2;
      continue;
    }
    if (marker === 0xda || marker === 0xd9) return { segments, scanAt: at };
    const length = u16be(bytes, at + 2);
    if (length < 2 || at + 2 + length > bytes.length) break;
    segments.push({ marker, at, length: length + 2 });
    at += 2 + length;
  }
  return { segments, scanAt: bytes.length };
}

/* APP1 is EXIF and XMP, APP13 is the Photoshop block that carries IPTC, and a
 * comment is free text somebody's software wrote. APP0 (JFIF) says how to read
 * the pixels and APP2 carries the colour profile, so both stay: dropping the
 * profile would change what the picture looks like, which is a different act
 * from taking the camera's notes out of it. */
const JPEG_KEPT_APP = new Set([0xe0, 0xe2]);

const dropJpeg = (marker: number): boolean =>
  marker === 0xfe || (marker >= 0xe0 && marker <= 0xef && !JPEG_KEPT_APP.has(marker));

/* tEXt, zTXt, iTXt and eXIf are where a PNG keeps everything that is not the
 * picture; tIME is the moment the file was last touched. All five are
 * ancillary, so every decoder is required to manage without them. */
const PNG_DROPPED = new Set(['tEXt', 'zTXt', 'iTXt', 'eXIf', 'tIME']);

/* A WebP keeps its metadata in two chunks of an extended-format file. */
const WEBP_DROPPED = new Set(['EXIF', 'XMP ']);

function concat(parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

function stripJpeg(bytes: Uint8Array): Uint8Array {
  const { segments, scanAt } = jpegSegments(bytes);
  const parts: Uint8Array[] = [bytes.subarray(0, 2)];
  for (const segment of segments) {
    if (dropJpeg(segment.marker)) continue;
    parts.push(bytes.subarray(segment.at, segment.at + segment.length));
  }
  parts.push(bytes.subarray(scanAt));
  return concat(parts);
}

function stripPng(bytes: Uint8Array): Uint8Array {
  const parts: Uint8Array[] = [bytes.subarray(0, 8)];
  let at = 8;
  while (at + 8 <= bytes.length) {
    const length = u32be(bytes, at);
    const type = ascii(bytes, at + 4, 4);
    const total = length + 12;
    if (total < 12 || at + total > bytes.length) break;
    if (!PNG_DROPPED.has(type)) parts.push(bytes.subarray(at, at + total));
    at += total;
    if (type === 'IEND') break;
  }
  return concat(parts);
}

function stripWebp(bytes: Uint8Array): Uint8Array {
  const parts: Uint8Array[] = [];
  let at = 12;
  let dropped = false;
  while (at + 8 <= bytes.length) {
    const type = ascii(bytes, at, 4);
    const size = u32le(bytes, at + 4);
    /* Chunks are padded to an even length and the pad byte is not counted. */
    const total = 8 + size + (size % 2);
    if (at + total > bytes.length) break;
    if (WEBP_DROPPED.has(type)) dropped = true;
    else parts.push(bytes.subarray(at, at + total));
    at += total;
  }
  if (!dropped) return bytes;
  const body = concat(parts);
  const head = new Uint8Array(12);
  head.set(bytes.subarray(0, 12));
  /* The RIFF length counts everything after its own four bytes, which is the
   * four of 'WEBP' plus the chunks that are left. */
  const size = body.length + 4;
  head[4] = size & 0xff;
  head[5] = (size >> 8) & 0xff;
  head[6] = (size >> 16) & 0xff;
  head[7] = (size >> 24) & 0xff;
  return concat([head, body]);
}

/** The same picture with nothing in it but the picture. The compressed pixels
 *  are never decoded, so what comes back is what the guest chose. */
export function stripMetadata(bytes: Uint8Array, mime: Allowed): Uint8Array {
  if (mime === 'image/jpeg') return stripJpeg(bytes);
  if (mime === 'image/png') return stripPng(bytes);
  return stripWebp(bytes);
}

/* SOF0 through SOF15 are the frame headers; 0xC4, 0xC8 and 0xCC share the
 * range and are a Huffman table, an extension and an arithmetic-coding table. */
const isSof = (marker: number): boolean =>
  marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc;

function jpegSize(bytes: Uint8Array): Size | null {
  for (const segment of jpegSegments(bytes).segments) {
    if (!isSof(segment.marker)) continue;
    return {
      height: u16be(bytes, segment.at + 5),
      width: u16be(bytes, segment.at + 7),
    };
  }
  return null;
}

function pngSize(bytes: Uint8Array): Size | null {
  if (bytes.length < 24 || ascii(bytes, 12, 4) !== 'IHDR') return null;
  return { width: u32be(bytes, 16), height: u32be(bytes, 20) };
}

function webpSize(bytes: Uint8Array): Size | null {
  let at = 12;
  while (at + 8 <= bytes.length) {
    const type = ascii(bytes, at, 4);
    const size = u32le(bytes, at + 4);
    const payload = at + 8;
    if (payload + size > bytes.length) return null;
    /* The canvas of an extended file is the whole answer and comes first. */
    if (type === 'VP8X' && size >= 10) {
      const width = byte(bytes, payload + 4) + (byte(bytes, payload + 5) << 8) + (byte(bytes, payload + 6) << 16);
      const height = byte(bytes, payload + 7) + (byte(bytes, payload + 8) << 8) + (byte(bytes, payload + 9) << 16);
      return { width: width + 1, height: height + 1 };
    }
    /* A lossy frame: a three-byte tag, a three-byte start code, then two
     * fourteen-bit dimensions. */
    if (type === 'VP8 ' && size >= 10) {
      return {
        width: u16le(bytes, payload + 6) & 0x3fff,
        height: u16le(bytes, payload + 8) & 0x3fff,
      };
    }
    /* A lossless frame packs both dimensions, less one, into 28 bits after a
     * one-byte signature. */
    if (type === 'VP8L' && size >= 5 && byte(bytes, payload) === 0x2f) {
      const packed = u32le(bytes, payload + 1);
      return { width: (packed & 0x3fff) + 1, height: ((packed >> 14) & 0x3fff) + 1 };
    }
    at += 8 + size + (size % 2);
  }
  return null;
}

/** The picture's own idea of how big it is, read out of its header. Null when
 *  the header does not say, which is a file the gallery will not lay out and
 *  therefore will not take. */
export function dimensionsOf(bytes: Uint8Array, mime: Allowed): Size | null {
  const size =
    mime === 'image/jpeg' ? jpegSize(bytes) : mime === 'image/png' ? pngSize(bytes) : webpSize(bytes);
  if (!size || size.width <= 0 || size.height <= 0) return null;
  return size;
}
