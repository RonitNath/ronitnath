import { describe, expect, it } from 'vitest';

import { dimensionsOf, isAllowed, isHeic, sniff, stripMetadata } from '../image';

/* The fixtures are built by hand rather than committed as files: what is
 * under test is a walk over headers, so a header is the whole fixture. None
 * of them carries a valid checksum or decodable pixels, and none needs to —
 * nothing here decodes an image, which is exactly the property being kept. */

const bytes = (...parts: (number | number[] | string)[]): Uint8Array => {
  const flat: number[] = [];
  for (const part of parts) {
    if (typeof part === 'string') for (const ch of part) flat.push(ch.charCodeAt(0));
    else if (Array.isArray(part)) flat.push(...part);
    else flat.push(part);
  }
  return new Uint8Array(flat);
};

const u16 = (n: number): number[] => [(n >> 8) & 0xff, n & 0xff];
const u32be = (n: number): number[] => [(n >> 24) & 0xff, (n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
const u32le = (n: number): number[] => [n & 0xff, (n >> 8) & 0xff, (n >> 16) & 0xff, (n >> 24) & 0xff];

/** A JPEG whose EXIF says where the photograph was taken. */
const jpeg = () =>
  bytes(
    [0xff, 0xd8],
    // APP1: the camera's notes, with a recognisable needle in them.
    [0xff, 0xe1],
    u16(2 + 6 + 8),
    'Exif',
    [0, 0],
    'GPSHERE',
    [0],
    // APP0: JFIF, which says how to read the pixels and stays.
    [0xff, 0xe0],
    u16(2 + 5),
    'JFIF',
    [0],
    // A comment somebody's software wrote.
    [0xff, 0xfe],
    u16(2 + 4),
    'note',
    // SOF0: precision, height, width, components.
    [0xff, 0xc0],
    u16(2 + 6),
    [8],
    u16(480),
    u16(640),
    [1],
    // The scan, and one 0xFF inside it that is not a marker.
    [0xff, 0xda],
    u16(2 + 3),
    [1, 0, 0],
    [0x12, 0xff, 0x00, 0x34],
    [0xff, 0xd9],
  );

const pngChunk = (type: string, payload: number[]): number[] => [
  ...u32be(payload.length),
  ...[...type].map((ch) => ch.charCodeAt(0)),
  ...payload,
  ...u32be(0),
];

const png = () =>
  bytes(
    [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
    pngChunk('IHDR', [...u32be(800), ...u32be(600), 8, 6, 0, 0, 0]),
    pngChunk('tEXt', [...'Location'].map((ch) => ch.charCodeAt(0))),
    pngChunk('eXIf', [1, 2, 3, 4]),
    pngChunk('IDAT', [9, 9, 9]),
    pngChunk('IEND', []),
  );

const webpChunk = (type: string, payload: number[]): number[] => {
  const pad = payload.length % 2 === 1 ? [0] : [];
  return [...[...type].map((ch) => ch.charCodeAt(0)), ...u32le(payload.length), ...payload, ...pad];
};

const webp = () => {
  const body = [
    ...webpChunk('VP8X', [0, 0, 0, 0, 0x3f, 0, 0, 0x2f, 0, 0]),
    ...webpChunk('EXIF', [7, 7, 7, 7]),
    ...webpChunk('VP8 ', [0, 0, 0, 0x9d, 0x01, 0x2a, 0x40, 0x00, 0x30, 0x00]),
  ];
  return bytes('RIFF', u32le(body.length + 4), 'WEBP', body);
};

const has = (haystack: Uint8Array, needle: string): boolean =>
  Buffer.from(haystack).includes(needle);

describe('the formats a browser may send', () => {
  it('takes three and refuses a HEIC by name', () => {
    expect(isAllowed('image/jpeg')).toBe(true);
    expect(isAllowed('image/png')).toBe(true);
    expect(isAllowed('image/webp')).toBe(true);
    expect(isAllowed('image/heic')).toBe(false);
    expect(isHeic('image/heic')).toBe(true);
    expect(isHeic('image/heif')).toBe(true);
    expect(isHeic('image/gif')).toBe(false);
  });
});

describe('sniff', () => {
  it('answers from the file rather than from the browser', () => {
    expect(sniff(jpeg())).toBe('image/jpeg');
    expect(sniff(png())).toBe('image/png');
    expect(sniff(webp())).toBe('image/webp');
  });

  it('refuses anything that is not one of the three', () => {
    expect(sniff(bytes('GIF89a', [0, 0, 0, 0, 0, 0]))).toBeNull();
    expect(sniff(bytes('#!/bin/sh\n'))).toBeNull();
    expect(sniff(new Uint8Array(0))).toBeNull();
  });
});

describe('stripMetadata', () => {
  it('takes the camera notes out of a JPEG and leaves the picture', () => {
    const before = jpeg();
    expect(has(before, 'GPSHERE')).toBe(true);
    const after = stripMetadata(before, 'image/jpeg');
    expect(has(after, 'GPSHERE')).toBe(false);
    expect(has(after, 'note')).toBe(false);
    /* JFIF says how to read the pixels; the scan is the picture. */
    expect(has(after, 'JFIF')).toBe(true);
    /* The scan is copied verbatim, 0xFF bytes inside it included. */
    expect([...after.slice(-6)]).toEqual([0x12, 0xff, 0x00, 0x34, 0xff, 0xd9]);
    expect(dimensionsOf(after, 'image/jpeg')).toEqual({ width: 640, height: 480 });
  });

  it('takes the text and EXIF chunks out of a PNG', () => {
    const after = stripMetadata(png(), 'image/png');
    expect(has(after, 'tEXt')).toBe(false);
    expect(has(after, 'eXIf')).toBe(false);
    expect(has(after, 'IHDR')).toBe(true);
    expect(has(after, 'IDAT')).toBe(true);
    expect(has(after, 'IEND')).toBe(true);
    expect(dimensionsOf(after, 'image/png')).toEqual({ width: 800, height: 600 });
  });

  it('takes the EXIF chunk out of a WebP and restates the RIFF length', () => {
    const after = stripMetadata(webp(), 'image/webp');
    expect(has(after, 'EXIF')).toBe(false);
    expect(has(after, 'VP8X')).toBe(true);
    const view = new DataView(after.buffer, after.byteOffset, after.byteLength);
    const stated = view.getUint32(4, true);
    expect(stated).toBe(after.length - 8);
    expect(dimensionsOf(after, 'image/webp')).toEqual({ width: 64, height: 48 });
  });

  it('leaves a file that has nothing to take out exactly as it was', () => {
    const clean = stripMetadata(png(), 'image/png');
    expect(stripMetadata(clean, 'image/png')).toEqual(clean);
  });
});

describe('dimensionsOf', () => {
  it('reads a lossy WebP frame when there is no canvas chunk', () => {
    const body = webpChunk('VP8 ', [0, 0, 0, 0x9d, 0x01, 0x2a, 0x40, 0x00, 0x30, 0x00]);
    const file = bytes('RIFF', u32le(body.length + 4), 'WEBP', body);
    expect(dimensionsOf(file, 'image/webp')).toEqual({ width: 64, height: 48 });
  });

  it('reads a lossless WebP frame', () => {
    const packed = (64 - 1) | ((48 - 1) << 14);
    const body = webpChunk('VP8L', [0x2f, ...u32le(packed)]);
    const file = bytes('RIFF', u32le(body.length + 4), 'WEBP', body);
    expect(dimensionsOf(file, 'image/webp')).toEqual({ width: 64, height: 48 });
  });

  it('is null when the header does not say', () => {
    expect(dimensionsOf(bytes([0xff, 0xd8, 0xff, 0xd9]), 'image/jpeg')).toBeNull();
    expect(dimensionsOf(bytes([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), 'image/png')).toBeNull();
  });
});
