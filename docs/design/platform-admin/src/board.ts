// Diagram board: state machines, sequence (call-waterfall) diagrams, topology,
// UI mocks. Labels only — the prose lives in the HTML report beside this.
import {
  createShapeId,
  createBindingId,
  toRichText,
  type TLShapePartial,
  type TLBindingCreate,
  type TLDefaultColorStyle,
  type TLGeoShape,
  type TLTextShape,
  type TLFrameShape,
  type TLArrowShape,
  type TLLineShape,
} from 'tldraw'

export type Built = { shapes: TLShapePartial[]; bindings: TLBindingCreate[] }

const C = {
  node: 'green',        // a serving process
  cmd: 'violet',        // a kernel command
  feed: 'orange',       // the change feed / audit row
  person: 'blue',       // a person, an identity, a session
  op: 'red',            // the operator, and everything god mode touches
  ui: 'light-blue',     // a rendered control
  ink: 'black',
  faint: 'grey',
} satisfies Record<string, TLDefaultColorStyle>

type Size = 's' | 'm' | 'l' | 'xl'
type Font = 'draw' | 'sans' | 'serif' | 'mono'
type Dash = 'draw' | 'solid' | 'dashed' | 'dotted'

const shapes: TLShapePartial[] = []
const bindings: TLBindingCreate[] = []
const ids = new Map<string, ReturnType<typeof createShapeId>>()
const id = (key: string) => {
  let v = ids.get(key)
  if (!v) {
    v = createShapeId(key.replace(/[^a-z0-9_-]/gi, '_'))
    ids.set(key, v)
  }
  return v
}
let n = 0
const fresh = () => id(`auto${n++}`)

const CHAR: Record<Size, Record<'mono' | 'sans', number>> = {
  s: { mono: 11, sans: 9 },
  m: { mono: 14, sans: 12 },
  l: { mono: 18, sans: 16 },
  xl: { mono: 24, sans: 22 },
}
const LINE: Record<Size, number> = { s: 25, m: 31, l: 40, xl: 52 }

function frame(key: string, x: number, y: number, w: number, h: number, name: string) {
  shapes.push({ id: id(key), type: 'frame', x, y, props: { w, h, name } } as TLShapePartial<TLFrameShape>)
}

type BoxOpts = {
  parent?: string
  x: number
  y: number
  w?: number
  h?: number
  color?: TLDefaultColorStyle
  dash?: Dash
  fill?: 'none' | 'semi' | 'solid' | 'pattern' | 'fill'
  font?: Font
  size?: Size
  geo?: TLGeoShape['props']['geo']
  align?: 'start' | 'middle' | 'end'
  valign?: 'start' | 'middle' | 'end'
  opacity?: number
}

function box(key: string, text: string, o: BoxOpts) {
  const size = o.size ?? 's'
  const font = o.font ?? 'sans'
  const rows = text.split('\n')
  const longest = Math.max(...rows.map((r) => r.length))
  const charW = CHAR[size][font === 'mono' ? 'mono' : 'sans']
  const autoW = longest * charW + 44
  const autoH = rows.length * LINE[size] + 32
  const w = o.w ?? autoW
  const h = o.h ?? autoH
  shapes.push({
    id: id(key),
    type: 'geo',
    parentId: o.parent ? id(o.parent) : undefined,
    x: o.x,
    y: o.y,
    opacity: o.opacity ?? 1,
    props: {
      w: Math.max(w, o.geo === 'ellipse' ? autoW * 1.15 : w),
      h: Math.max(h, o.geo === 'ellipse' ? autoH * 1.3 : h),
      geo: o.geo ?? 'rectangle',
      color: o.color ?? C.ink,
      labelColor: 'black',
      fill: o.fill ?? 'semi',
      dash: o.dash ?? 'solid',
      size,
      font,
      align: o.align ?? 'middle',
      verticalAlign: o.valign ?? 'middle',
      richText: toRichText(text),
    },
  } as TLShapePartial<TLGeoShape>)
  return key
}

function label(key: string, text: string, o: { parent?: string; x: number; y: number; size?: Size; color?: TLDefaultColorStyle; font?: Font }) {
  shapes.push({
    id: id(key),
    type: 'text',
    parentId: o.parent ? id(o.parent) : undefined,
    x: o.x,
    y: o.y,
    props: { w: 400, autoSize: true, size: o.size ?? 's', font: o.font ?? 'sans', color: o.color ?? C.ink, textAlign: 'start', richText: toRichText(text) },
  } as TLShapePartial<TLTextShape>)
}

// Bound arrow between two shapes (edge-to-edge routing by tldraw).
function link(from: string, to: string, o: { text?: string; color?: TLDefaultColorStyle; dash?: Dash; bend?: number; a?: { x: number; y: number }; b?: { x: number; y: number }; parent?: string } = {}) {
  const aid = fresh()
  shapes.push({
    id: aid,
    type: 'arrow',
    parentId: o.parent ? id(o.parent) : undefined,
    x: 0,
    y: 0,
    props: { color: o.color ?? C.ink, dash: o.dash ?? 'solid', size: 's', font: 'sans', arrowheadStart: 'none', arrowheadEnd: 'arrow', bend: o.bend ?? 0, richText: toRichText(o.text ?? ''), start: { x: 0, y: 0 }, end: { x: 100, y: 100 } },
  } as TLShapePartial<TLArrowShape>)
  for (const [terminal, target, anchor] of [['start', from, o.a], ['end', to, o.b]] as const) {
    bindings.push({
      id: createBindingId(),
      type: 'arrow',
      fromId: aid,
      toId: id(target),
      props: { terminal, normalizedAnchor: anchor ?? { x: 0.5, y: 0.5 }, isExact: false, isPrecise: !!anchor, snap: 'none' },
    } as TLBindingCreate)
  }
}

// Free arrow with explicit points (frame-relative when parent is given).
function arrowAt(parent: string | undefined, x1: number, y1: number, x2: number, y2: number, o: { text?: string; color?: TLDefaultColorStyle; dash?: Dash; bend?: number; headStart?: boolean; noHead?: boolean } = {}) {
  shapes.push({
    id: fresh(),
    type: 'arrow',
    parentId: parent ? id(parent) : undefined,
    x: 0,
    y: 0,
    props: { color: o.color ?? C.ink, dash: o.dash ?? 'solid', size: 's', font: 'sans', arrowheadStart: o.headStart ? 'arrow' : 'none', arrowheadEnd: o.noHead ? 'none' : 'arrow', bend: o.bend ?? 0, richText: toRichText(o.text ?? ''), start: { x: x1, y: y1 }, end: { x: x2, y: y2 } },
  } as TLShapePartial<TLArrowShape>)
}

function line(parent: string | undefined, x1: number, y1: number, x2: number, y2: number, o: { color?: TLDefaultColorStyle; dash?: Dash } = {}) {
  shapes.push({
    id: fresh(),
    type: 'line',
    parentId: parent ? id(parent) : undefined,
    x: x1,
    y: y1,
    props: {
      color: o.color ?? C.faint,
      dash: o.dash ?? 'dotted',
      size: 's',
      spline: 'line',
      scale: 1,
      points: { a1: { id: 'a1', index: 'a1', x: 0, y: 0 }, a2: { id: 'a2', index: 'a2', x: x2 - x1, y: y2 - y1 } },
    },
  } as unknown as TLShapePartial<TLLineShape>)
}

// ── State machine helper ────────────────────────────────────────────────────
type State = { key: string; text: string; x: number; y: number; color?: TLDefaultColorStyle; dash?: Dash; fill?: BoxOpts['fill']; terminal?: boolean }
type Trans = { from: string; to: string; text: string; bend?: number; dash?: Dash; color?: TLDefaultColorStyle; a?: { x: number; y: number }; b?: { x: number; y: number } }
function machine(parent: string, states: State[], trans: Trans[], color: TLDefaultColorStyle) {
  for (const s of states) {
    box(s.key, s.text, { parent, x: s.x, y: s.y, geo: s.terminal ? 'rectangle' : 'ellipse', color: s.color ?? color, dash: s.dash, fill: s.fill ?? (s.terminal ? 'solid' : 'semi'), size: 's' })
  }
  for (const t of trans) {
    if (t.from === t.to) {
      // self-loop: free arrow arc hung off the state's top edge
      const s = states.find((x) => x.key === t.from)!
      const rows = s.text.split('\n')
      const w = Math.max(...rows.map((r) => r.length)) * 9 * 1.15 + 44
      const cx = s.x + w / 2
      arrowAt(parent, cx - 40, s.y + 4, cx + 40, s.y + 4, { text: t.text, color: t.color ?? color, dash: t.dash, bend: -90 })
    } else {
      link(t.from, t.to, { parent, text: t.text, color: t.color ?? color, dash: t.dash, bend: t.bend, a: t.a, b: t.b })
    }
  }
}

// ── Sequence (call-waterfall) helper ─────────────────────────────────────────
// Labels are separate text shapes above each arrow: an arrow's own label wraps
// to the arrow's length, which mangles anything longer than two words.
type Msg = { from: number; to: number; text: string; dash?: Dash; color?: TLDefaultColorStyle; note?: string; gap?: number }
function sequence(parent: string, participants: { text: string; color?: TLDefaultColorStyle }[], msgs: Msg[], o: { x0?: number; y0?: number; colW?: number; step?: number } = {}) {
  const x0 = o.x0 ?? 60
  const y0 = o.y0 ?? 50
  const colW = o.colW ?? 260
  const step = o.step ?? 70
  const xs = participants.map((_, i) => x0 + i * colW)
  const rowsY = y0 + 80
  const endY = rowsY + msgs.reduce((acc, m) => acc + (m.gap ?? 0) + step, 0) + 40
  participants.forEach((p, i) => {
    box(`${parent}.p${i}`, p.text, { parent, x: xs[i] - 100, y: y0, w: 200, h: 56, color: p.color ?? C.ink, fill: 'solid', size: 's' })
    line(parent, xs[i], y0 + 56, xs[i], endY)
  })
  let y = rowsY
  msgs.forEach((m, i) => {
    y += m.gap ?? 0
    if (i > 0 && msgs[i - 1].from === msgs[i - 1].to) y += 24 // clear the self-message arc
    const x1 = xs[m.from]
    const x2 = xs[m.to]
    if (m.from === m.to) {
      arrowAt(parent, x1 + 3, y, x1 + 3, y + 40, { color: m.color ?? C.ink, dash: m.dash, bend: -32 })
      label(`${parent}.l${i}`, m.text, { parent, x: x1 + 44, y: y + 6, color: m.color ?? C.ink })
    } else {
      arrowAt(parent, x1 + (x2 > x1 ? 4 : -4), y, x2 + (x2 > x1 ? -4 : 4), y, { color: m.color ?? C.ink, dash: m.dash })
      const w = m.text.length * 9
      label(`${parent}.l${i}`, m.text, { parent, x: (x1 + x2) / 2 - w / 2, y: y - 30, color: m.color ?? C.ink })
    }
    if (m.note) label(`${parent}.n${i}`, m.note, { parent, x: Math.max(x1, x2) + 30, y: y - 12, color: C.faint })
    y += step
  })
  return endY
}

// ── UI mock helper ───────────────────────────────────────────────────────────
function screen(key: string, parent: string, x: number, y: number, w: number, h: number, title: string) {
  box(`${key}.win`, '', { parent, x, y, w, h, color: C.ink, fill: 'none' })
  box(`${key}.bar`, title, { parent, x, y, w, h: 36, color: C.faint, fill: 'solid', size: 's', font: 'mono', align: 'start' })
}
function ui(key: string, parent: string, x: number, y: number, w: number, h: number, text: string, o: { color?: TLDefaultColorStyle; fill?: BoxOpts['fill']; dash?: Dash; align?: 'start' | 'middle'; top?: boolean; mono?: boolean } = {}) {
  box(key, text, {
    parent, x, y, w, h,
    color: o.color ?? C.faint,
    fill: o.fill ?? 'none',
    dash: o.dash,
    size: 's',
    font: o.mono ? 'mono' : 'sans',
    align: o.align ?? 'middle',
    // A block of rows reads from its top edge; a single centred phrase does not.
    valign: o.top || o.align === 'start' ? 'start' : 'middle',
  })
}

// ═══════════════════════════════════════════════════════════════════════════
// Platform admin — the diagrams. Prose is in public/index.html beside this.
// ═══════════════════════════════════════════════════════════════════════════

// ── A1 · deployment topology ────────────────────────────────────────────────
const TOP = 'top'
frame(TOP, 0, 0, 1600, 900, 'A1 · Topology — three voters, one feed')

box('top.edge', 'edge\n(one hostname)', { parent: TOP, x: 640, y: 50, w: 300, h: 86, color: C.faint, fill: 'solid' })
for (const [i, name] of ['nexus', 'nyc', 'delenda'].entries()) {
  const x = 120 + i * 470
  box(`top.n${i}`, `node · ${name}\nrn-site`, { parent: TOP, x, y: 250, w: 340, h: 96, color: C.node, fill: 'solid' })
  box(`top.p${i}`, 'ProductSet\n(ArcSwap projection)', { parent: TOP, x: x + 20, y: 400, w: 300, h: 86, color: C.cmd })
  box(`top.d${i}`, 'hiqlite voter\naudit = the feed', { parent: TOP, x: x + 20, y: 560, w: 300, h: 86, color: C.feed })
  link('top.edge', `top.n${i}`, { parent: TOP })
  link(`top.n${i}`, `top.p${i}`, { parent: TOP })
  link(`top.d${i}`, `top.p${i}`, { parent: TOP, text: 'invalidate', color: C.feed, dash: 'dashed' })
}
arrowAt(TOP, 460, 604, 620, 604, { text: 'raft', color: C.feed, headStart: true })
arrowAt(TOP, 930, 604, 1090, 604, { text: 'raft', color: C.feed, headStart: true })
label('top.note', 'One raft group for the store, one for the cache.\nA command committed on any node is on all three.', { parent: TOP, x: 120, y: 710, color: C.faint })
label('top.note2', 'The projection is per node and holds no truth of its own:\nit is re-read from the product table when the feed says so.', { parent: TOP, x: 780, y: 710, color: C.faint })

// ── A2 · product enablement propagates ──────────────────────────────────────
const PROP = 'prop'
frame(PROP, 1750, 0, 1740, 1080, 'A2 · Sequence — a product toggle reaches every node, no restart')
sequence(PROP, [
  { text: '/platform\nProducts', color: C.ui },
  { text: 'node 1', color: C.node },
  { text: 'raft\n(audit)', color: C.feed },
  { text: 'node 2', color: C.node },
  { text: "node 2\nProductSet", color: C.cmd },
], [
  { from: 0, to: 1, text: 'POST /api/cmd/enable-product', gap: 40 },
  { from: 1, to: 1, text: 'operator? re-auth window?', color: C.op },
  { from: 1, to: 2, text: 'one txn: product row + audit row' },
  { from: 2, to: 1, text: 'offset', dash: 'dashed' },
  { from: 1, to: 0, text: '200 { offset }', dash: 'dashed', note: 'the caller now has a barrier' },
  { from: 2, to: 3, text: 'ProductEnabled', color: C.feed },
  { from: 3, to: 4, text: 'SELECT product; swap', color: C.cmd },
  { from: 4, to: 3, text: 'gate now says on', dash: 'dashed', color: C.cmd },
  { from: 3, to: 3, text: 'same pid, new answer', color: C.node },
], { x0: 240, colW: 340, step: 74 })
label('prop.why', 'The router is built once at boot, so no route is added here.\nEvery product route is mounted always and wrapped in one gate\nthat reads the projection and answers 404 when the product is off —\nindistinguishable from a product this deployment never had.', { parent: PROP, x: 240, y: 900, color: C.faint })

// ── B1 · impersonation, as a state machine ──────────────────────────────────
const IMP = 'imp'
frame(IMP, 0, 1180, 1600, 880, 'B1 · State — an operator session and the one it opens')
machine(IMP, [
  { key: 'imp.own', text: 'operator\nsession', x: 90, y: 120, color: C.op },
  { key: 'imp.as', text: 'impersonating\nB', x: 640, y: 120, color: C.op },
  { key: 'imp.gone', text: 'no session', x: 1200, y: 120, terminal: true, color: C.faint },
], [
  { from: 'imp.own', to: 'imp.as', text: 'SignInAs' },
  { from: 'imp.as', to: 'imp.own', text: 'EndImpersonation', bend: 80 },
  { from: 'imp.as', to: 'imp.gone', text: '30 min' },
], C.op)
arrowAt(IMP, 661, 124, 741, 124, { bend: -90, color: C.op })
label('imp.self', "while it lasts, every command in that session\nruns on B's own subject set — check() learns\nno new case", { parent: IMP, x: 640, y: 250, color: C.op })
label('imp.ends', 'Four things end it, and only the first is a click:\n  EndImpersonation\n  IMPERSONATION_TTL, 30 minutes\n  the operator loses platform:* #operator\n  B is disabled, or B revokes the session', { parent: IMP, x: 90, y: 460, color: C.ink })
label('imp.keep', "The operator's own session is never touched, so\n'back' is a cookie they still hold, not a re-login.", { parent: IMP, x: 700, y: 460, color: C.faint })
label('imp.no', 'Refused while impersonating: SignInAs, GrantOperator, RevokeOperator,\nAddFactor, RemoveFactor, SetHandle, Disable, RegisterClient,\nRotateClientSecret, ReAuthenticate. An impersonated session may not\nchange what the person is, nor who may become them.', { parent: IMP, x: 90, y: 660, color: C.op })

// ── B2 · sign-in-as, as a sequence ──────────────────────────────────────────
const SIA = 'sia'
frame(SIA, 1750, 1180, 1740, 1080, 'B2 · Sequence — sign in as a person, and what the audit says')
sequence(SIA, [
  { text: 'operator\nbrowser', color: C.op },
  { text: '/api/cmd', color: C.node },
  { text: 'kernel\ncommand', color: C.cmd },
  { text: 'session\nrow', color: C.person },
  { text: 'audit\nrow', color: C.feed },
], [
  { from: 0, to: 1, text: 'SignInAs { person: B, reason }', gap: 40 },
  { from: 1, to: 2, text: 'operator? not another operator? re-auth?', color: C.op },
  { from: 2, to: 3, text: "mint for B's identity — impersonated_by = the operator" },
  { from: 2, to: 4, text: 'actor = operator, hat = B', color: C.feed },
  { from: 2, to: 0, text: 'Set-Cookie: rn_session', dash: 'dashed' },
  { from: 0, to: 1, text: 'every later command in that session', gap: 30 },
  { from: 1, to: 2, text: "check() over B's subject set", color: C.person },
  { from: 2, to: 4, text: 'refs::actor — one function, every command', color: C.feed },
], { x0: 240, colW: 340, step: 74 })
label('sia.view', "B's own view: GET /api/q/about-me carries the row,\nnamed with the operator's display name.\nThe operator's chrome carries a bar, not a pill, while it lasts.", { parent: SIA, x: 240, y: 900, color: C.faint })

// ── C · operator delegation ─────────────────────────────────────────────────
const DEL = 'del'
frame(DEL, 0, 2400, 1600, 880, 'C · Operator delegation — the first one, and every one after')
machine(DEL, [
  { key: 'del.none', text: 'no operator', x: 80, y: 120, fill: 'none', color: C.faint },
  { key: 'del.first', text: 'one operator', x: 620, y: 120, color: C.op },
  { key: 'del.many', text: 'several', x: 1180, y: 120, color: C.op },
], [
  { from: 'del.none', to: 'del.first', text: 'bootstrap' },
  { from: 'del.first', to: 'del.many', text: 'GrantOperator' },
  { from: 'del.many', to: 'del.first', text: 'RevokeOperator', bend: 70 },
], C.op)
arrowAt(DEL, 1201, 124, 1281, 124, { bend: -90, color: C.op })
label('del.self', 'GrantOperator, again', { parent: DEL, x: 1150, y: 215, color: C.op })
box('del.boot', 'bootstrap_operator\nno actor, no audit row, refuses the second', { parent: DEL, x: 80, y: 380, w: 620, h: 100, color: C.faint, dash: 'dashed' })
box('del.cmd', 'GrantOperator / RevokeOperator\nactor + reason + audit row', { parent: DEL, x: 800, y: 380, w: 620, h: 100, color: C.cmd })
label('del.why', 'Unaudited is defensible as the act that creates the first operator\nout of nothing, and indefensible as the act that creates the second.', { parent: DEL, x: 80, y: 520, color: C.faint })
label('del.last', 'RevokeOperator refuses the last one — a deployment with no operator is one\nnobody can rule on, delegate from, or unwedge. Today neither command exists:\nSetRole writes membership rows and platform is not a container, so story A2\nhas no implementation at all.', { parent: DEL, x: 80, y: 650, color: C.op })

// ── D · disable / enable cascade ────────────────────────────────────────────
const DIS = 'dis'
frame(DIS, 1750, 2400, 1740, 880, 'D · Disable and enable — what ends, and what comes back')
box('dis.party', 'party.status\nactive → disabled', { parent: DIS, x: 660, y: 60, w: 420, h: 96, color: C.person, fill: 'solid' })
const casc: [string, string, string, boolean][] = [
  ['dis.s', 'sessions', 'DELETE', true],
  ['dis.t', 'oidc_token', 'DELETE', true],
  ['dis.c', 'oidc_code', 'DELETE', true],
  ['dis.l', 'link', 'suspended_at ← now', false],
  ['dis.k', 'oidc_consent', 'kept', false],
  ['dis.r', 'resources,\nrelations', 'kept', false],
]
casc.forEach(([key, what, how, gone], i) => {
  const x = 40 + i * 282
  box(key, `${what}\n${how}`, { parent: DIS, x, y: 340, w: 262, h: 110, color: gone ? C.op : C.node, dash: how === 'kept' ? 'dashed' : 'solid' })
  link('dis.party', key, { parent: DIS, color: gone ? C.op : C.node, dash: how === 'kept' ? 'dashed' : 'solid' })
})
label('dis.today', 'Today the command does the first three. Nothing touches link, so a disabled\nperson’s outstanding invitations still claim — finding F3.', { parent: DIS, x: 40, y: 520, color: C.op })
label('dis.back', 'Enable flips the status back and clears suspended_at. Sessions and tokens do\nnot return, and should not: they are re-mintable by signing in. A link is not,\nwhich is why it is stamped rather than deleted — revoke_link is the command\nthat means gone for good.', { parent: DIS, x: 40, y: 640, color: C.faint })

// ── E1 · backup and restore ─────────────────────────────────────────────────
const BAK = 'bak'
frame(BAK, 0, 3420, 1600, 960, 'E1 · Backup and restore — a fresh formation, proven by the health screen')
sequence(BAK, [
  { text: 'rn-site admin', color: C.cmd },
  { text: 'ReadStore\n(one offset)', color: C.node },
  { text: 'manifest\n+ rows', color: C.feed },
  { text: 'empty\nformation', color: C.person },
], [
  { from: 0, to: 1, text: 'backup <dir>', gap: 40 },
  { from: 1, to: 2, text: 'walk every table in dependency order' },
  { from: 2, to: 0, text: 'offset, schema hash, id-key fingerprint, counts', dash: 'dashed' },
  { from: 0, to: 3, text: 'restore <dir>', gap: 40 },
  { from: 3, to: 3, text: 'refuses if not empty', color: C.op },
  { from: 3, to: 0, text: '/readyz feed head = the manifest offset', dash: 'dashed' },
], { x0: 240, colW: 340, step: 78 })
label('bak.spike', "The first commit of that leg is a finding, not a design: what hiqlite 0.14\nactually exposes for snapshot and restore. If it exposes nothing, the logical\nwalk above IS the design — portable across schema versions, and the same\nwalk E2 needs.", { parent: BAK, x: 240, y: 780, color: C.faint })

// ── E2 · export / import an organization ────────────────────────────────────
const EXP = 'exp'
frame(EXP, 1750, 3420, 1740, 960, 'E2 · Export and import an organization (deferred — the shape, so it is not guessed twice)')
box('exp.a', 'deployment A', { parent: EXP, x: 90, y: 110, w: 380, h: 76, color: C.node, fill: 'solid' })
box('exp.org', 'organization party\n+ every resource it owns\n+ every relation inside the subtree', { parent: EXP, x: 60, y: 260, w: 460, h: 130, color: C.person })
box('exp.unit', 'portable unit\nmanifest + rows + public-id map', { parent: EXP, x: 640, y: 275, w: 440, h: 100, color: C.feed })
box('exp.b', 'deployment B', { parent: EXP, x: 1220, y: 110, w: 380, h: 76, color: C.node, fill: 'solid' })
box('exp.new', 'new parties, new ids\nrewritten through the map', { parent: EXP, x: 1180, y: 265, w: 460, h: 120, color: C.person, dash: 'dashed' })
link('exp.a', 'exp.org', { parent: EXP })
link('exp.org', 'exp.unit', { parent: EXP, text: 'export' })
link('exp.unit', 'exp.new', { parent: EXP, text: 'import' })
link('exp.b', 'exp.new', { parent: EXP })
label('exp.rule', 'No id crosses a deployment boundary unrewritten: every deployment is its own\nidentity authority, and nothing federates between them.', { parent: EXP, x: 60, y: 480, color: C.op })
label('exp.block', 'Deferred until the full backup walk exists and a second product does.\nWith one product, "everything an organization owns" is documents, and the\nformat would be shaped by that accident.', { parent: EXP, x: 60, y: 620, color: C.faint })

// ── F · UI mocks, 1440 ──────────────────────────────────────────────────────
const U1 = 'u1'
frame(U1, 0, 4520, 1620, 1120, 'F1 · Deployment — 1440')
screen('u1.s', U1, 60, 60, 1440, 980, 'GET /platform  ·  Deployment')
ui('u1.rail', U1, 80, 130, 220, 890, 'Deployment\nProducts\nOperators\nParties\nIdentities\nSessions\nAudit\nMatches\nResources\nClients\nKeys', { align: 'start' })
ui('u1.h', U1, 320, 130, 1160, 60, 'Deployment', { color: C.ink, align: 'start' })
ui('u1.nodes', U1, 320, 210, 1160, 160, 'node       version   raft      feed head   last report\nnexus      9f21ac3   leader      412 903     2s ago\nnyc        9f21ac3   follower    412 903     3s ago\ndelenda    8b0e114   follower    412 774    41s ago', { color: C.node, align: 'start', mono: true })
ui('u1.roll', U1, 320, 390, 560, 70, 'Rollout in progress · two versions', { color: C.op })
ui('u1.feed', U1, 900, 390, 580, 70, 'Feed head 412 903 · 9 214 today', { color: C.feed })
ui('u1.raft', U1, 320, 480, 560, 150, 'store   3 of 3 voters   leader 1\ncache   3 of 3 voters   leader 1\nsubscribers 12\nobservations pending 0', { align: 'start', mono: true })
ui('u1.keys', U1, 900, 480, 580, 150, 'kid  status     age  tokens alive\n7f2  active      6d           418\n1a9  retiring   41d             3', { color: C.feed, align: 'start', mono: true })
ui('u1.rot', U1, 900, 650, 220, 60, '[ rotate key ]', { color: C.ui })
label('u1.note', 'Every number is a reading of something the process witnessed —\nnot a rate, not a projection, and not a chart of one node’s\nraft membership.', { parent: U1, x: 320, y: 760, color: C.faint })

const U2 = 'u2'
frame(U2, 1770, 4520, 1620, 1020, 'F2 · Products — 1440')
screen('u2.s', U2, 60, 60, 1440, 880, 'GET /platform/products')
ui('u2.rail', U2, 80, 130, 220, 790, 'Deployment\nProducts\nOperators\n…', { align: 'start' })
ui('u2.h', U2, 320, 130, 1160, 60, 'Products', { color: C.ink, align: 'start' })
ui('u2.t', U2, 320, 210, 1160, 190, 'product     routes                    state   changed\npresence    /now  /presence         on      Ronit, 3d ago\nevents      /events  /rsvp/*         off     Ronit, 12d ago\ndocuments   /app/documents           on      bootstrap', { color: C.cmd, align: 'start', mono: true })
ui('u2.tog', U2, 320, 430, 360, 70, 'events   [ turn on ]', { color: C.ui })
ui('u2.says', U2, 700, 430, 780, 70, 'Mounts /events and /rsvp/* on every node.', { color: C.faint })
label('u2.note', 'The routes column comes from the compiled-in catalogue, so the screen states\nwhat turning a product off will actually take away. A slug the binary does not\ncarry cannot be enabled at all — a typo is a refusal, not a dead row.', { parent: U2, x: 320, y: 560, color: C.faint })

const U3 = 'u3'
frame(U3, 3540, 4520, 1620, 1200, 'F3 · Person — everything the system knows — 1440')
screen('u3.s', U3, 60, 60, 1440, 1060, 'GET /platform/parties/p_8Qk…')
ui('u3.h', U3, 80, 130, 1400, 70, 'Bea Okafor   ·   @bea   ·   p_8Qk7…   ·   active', { color: C.ink, align: 'start' })
ui('u3.act', U3, 80, 220, 1400, 60, '[ sign in as ]   [ disable ]   [ transfer… ]   [ grant operator ]', { color: C.op })
ui('u3.id', U3, 80, 300, 690, 170, 'identities\nb…a@example.com    password, email   verified\nb…a@work.example   email             merged in', { align: 'start', color: C.person, mono: true })
ui('u3.se', U3, 790, 300, 690, 170, 'sessions\nfirefox / macos   2h ago    [revoke]\nios               4d ago    [revoke]', { align: 'start', color: C.person, mono: true })
ui('u3.me', U3, 80, 500, 690, 150, 'memberships\nIsoastra        admin\nSpring cohort   member', { align: 'start', mono: true })
ui('u3.co', U3, 790, 500, 690, 150, 'consents\noauth2-proxy   openid profile\n14d ago        [revoke]', { align: 'start', color: C.feed, mono: true })
ui('u3.ow', U3, 80, 680, 690, 140, 'owns\n7 documents\n1 group', { align: 'start', mono: true })
ui('u3.mg', U3, 790, 680, 690, 140, 'merge history\nabsorbed i_4Tz… operator 2026-07-14\n"support call, passport shown"', { align: 'start', color: C.op, mono: true })
label('u3.note', 'Addresses are masked. Revealing one is a control that writes its own\naudit row, because reading somebody’s address is a thing the operator did.', { parent: U3, x: 80, y: 900, color: C.faint })

const U4 = 'u4'
frame(U4, 0, 5880, 1620, 1060, 'F4 · Audit with filters — 1440')
screen('u4.s', U4, 60, 60, 1440, 900, 'GET /platform/audit')
ui('u4.f', U4, 80, 130, 1400, 60, 'actor ▾    hat ▾    object ▾    command ▾    from – to', { color: C.ui, align: 'start' })
ui('u4.t', U4, 80, 220, 1400, 200, 'offset   command          actor        acting as     at\n412903   enable-product   Ronit        Ronit         12:04:11\n412902   sign-in-as       Ronit        Bea Okafor    12:03:58\n412901   revoke-session   Ronit        Bea Okafor    12:03:44\n412900   rule-match       Ronit        Ronit         11:58:02\n412899   share            Bea Okafor   Isoastra      11:41:20', { align: 'start', color: C.feed, mono: true })
ui('u4.p', U4, 80, 450, 1400, 160, 'offset 412902 · sign-in-as\nreason: support ticket 4412, mailbox lost\nperson: p_8Qk7… (Bea Okafor)   session: s_2Lm… (ended 12:31)', { align: 'start', mono: true })
label('u4.note', 'Every filter is an index seek or it does not ship: an operator filtering the\ndeployment’s whole history must not scan it. The row expands to the typed\nevent its transaction wrote — the reasoning is attached to the change, not\nto a memory of it.', { parent: U4, x: 80, y: 700, color: C.faint })

const U5 = 'u5'
frame(U5, 1770, 5880, 1620, 720, 'F5 · The impersonation bar — 1440')
screen('u5.s', U5, 60, 60, 1440, 560, 'GET /app  ·  while impersonating')
ui('u5.bar', U5, 80, 130, 1400, 70, 'Signed in as Bea Okafor by Ronit Nath  ·  ends 12:31  ·  [ end now ]', { color: C.op, fill: 'solid' })
ui('u5.rail', U5, 80, 220, 220, 370, 'Home\nDocuments\nGroups\nSessions', { align: 'start' })
ui('u5.body', U5, 320, 220, 1160, 370, "Bea’s own /app, unchanged — her subject set, her rows.\nNothing here is filtered by the operator’s authority, because the\nsession is hers and check() never learns a new case.", { color: C.faint })
label('u5.note', 'A bar, not a toast and not a pill: it must still be there twenty minutes in.\nBea’s own record of it is /app → about-me, which lists rulings made about\nher whoever the actor was.', { parent: U5, x: 80, y: 640, color: C.faint })

// ── F6 · the same screens at 390 ────────────────────────────────────────────
const U6 = 'u6'
frame(U6, 3540, 5880, 1620, 1180, 'F6 · Phone — 390')
screen('u6.a', U6, 60, 70, 390, 980, 'Deployment')
ui('u6.a1', U6, 75, 130, 360, 60, '☰   Deployment', { color: C.ink, align: 'start' })
ui('u6.a2', U6, 75, 205, 360, 160, 'nexus    9f21ac3 leader\nnyc      9f21ac3 follower\ndelenda  8b0e114 follower\nreported 2s / 3s / 41s', { align: 'start', color: C.node, mono: true })
ui('u6.a3', U6, 75, 380, 360, 60, 'Rollout in progress', { color: C.op })
ui('u6.a4', U6, 75, 455, 360, 160, 'store 3 of 3 leader 1\ncache 3 of 3 leader 1\nfeed head 412 903\nsubscribers 12', { align: 'start', mono: true })
ui('u6.a5', U6, 75, 630, 360, 130, 'keys\n7f2 active   6d\n1a9 retiring 41d', { align: 'start', color: C.feed, mono: true })

screen('u6.b', U6, 530, 70, 390, 980, 'Person')
ui('u6.b1', U6, 545, 130, 360, 80, 'Bea Okafor\n@bea · active', { color: C.ink, align: 'start' })
ui('u6.b2', U6, 545, 225, 360, 100, '[ sign in as ]\n[ disable ]', { color: C.op })
ui('u6.b3', U6, 545, 340, 360, 140, 'identities (2)\nsessions (2)\nmemberships (2)\nconsents (1)', { align: 'start', color: C.person, mono: true })
ui('u6.b4', U6, 545, 495, 360, 110, 'owns\n7 documents\n1 group', { align: 'start', mono: true })
ui('u6.b5', U6, 545, 620, 360, 140, 'merge history\nabsorbed i_4Tz…\noperator 2026-07-14', { align: 'start', color: C.op, mono: true })

screen('u6.c', U6, 1000, 70, 390, 980, 'Impersonating')
ui('u6.c1', U6, 1015, 130, 360, 100, 'as Bea Okafor\nby Ronit · ends 12:31\n[ end now ]', { color: C.op, fill: 'solid' })
ui('u6.c2', U6, 1015, 245, 360, 60, '☰   Home', { align: 'start' })
ui('u6.c3', U6, 1015, 320, 360, 440, "Bea’s own /app", { color: C.faint })
label('u6.note', 'The rail becomes a drawer; the tables drop to their primary columns.\nThe bar keeps its full sentence — it is the one thing on the page that\nmust not be elided.', { parent: U6, x: 60, y: 1070, color: C.faint })

export const built: Built = { shapes, bindings }
