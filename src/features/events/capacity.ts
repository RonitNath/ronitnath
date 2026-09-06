/* Capacity. The host's number is a fact about the room, not a gate on the
 * form: a guest who says yes when the room is already full is still coming as
 * far as the model is concerned — they are told the word `full`, and the host
 * sees where the line was crossed. Refusing the answer would lose it.
 *
 * A yes counts the guest and their plus-ones; a maybe counts nobody, because
 * a maybe is not a seat until it is a yes. */

export interface HeadcountRow {
  response: 'yes' | 'maybe' | 'no';
  plusOne: number;
}

export interface Headcount {
  yes: number;
  maybe: number;
  no: number;
  /* Bodies, not answers: every yes plus every plus-one it brought. */
  coming: number;
}

export function headcount(rows: readonly HeadcountRow[]): Headcount {
  const out: Headcount = { yes: 0, maybe: 0, no: 0, coming: 0 };
  for (const row of rows) {
    out[row.response] += 1;
    if (row.response === 'yes') out.coming += 1 + Math.max(0, row.plusOne);
  }
  return out;
}

export type Fullness = 'open' | 'full';

/** Whether the room is full, counting a prospective answer that is not in the
 *  rows yet. Null capacity is never full. */
export function fullness(
  capacity: number | null,
  rows: readonly HeadcountRow[],
  incoming?: HeadcountRow,
): Fullness {
  if (capacity === null) return 'open';
  const total = headcount(incoming ? [...rows, incoming] : rows).coming;
  return total >= capacity ? 'full' : 'open';
}

/** How many bodies are past the line, for the host's table. Zero when the
 *  room is open or exactly full. */
export function overflow(capacity: number | null, rows: readonly HeadcountRow[]): number {
  if (capacity === null) return 0;
  return Math.max(0, headcount(rows).coming - capacity);
}
