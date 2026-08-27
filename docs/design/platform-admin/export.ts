// Headless .tldr export of the same records the page builds.
import { createTLStore, defaultShapeUtils, defaultBindingUtils, getSnapshot, getIndices, TLPageId } from 'tldraw'
import { built } from './src/board'
import { writeFileSync } from 'node:fs'

const store = createTLStore({ shapeUtils: defaultShapeUtils, bindingUtils: defaultBindingUtils })
store.ensureStoreIsUsable()
const pageId = store.allRecords().find((r) => r.typeName === 'page')!.id as TLPageId
const shapeIndexes = new Map<string, number>()
const indices = getIndices(built.shapes.length + 1)
const now = built.shapes.map((s, i) => {
  const parentId = (s.parentId ?? pageId) as string
  const n = (shapeIndexes.get(parentId) ?? 0) + 1
  shapeIndexes.set(parentId, n)
  const util = defaultShapeUtils.find((u) => u.type === s.type)!
  const inst = new (util as any)(null)
  return {
    id: s.id,
    typeName: 'shape',
    type: s.type,
    x: s.x ?? 0,
    y: s.y ?? 0,
    rotation: 0,
    isLocked: false,
    opacity: 1,
    meta: {},
    parentId: s.parentId ?? pageId,
    index: indices[n],
    props: { ...inst.getDefaultProps(), ...s.props },
  }
})
store.put(now as any)
store.put(built.bindings.map((b) => ({ ...b, typeName: 'binding', meta: {} })) as any)
const snap = getSnapshot(store)
const file = { tldrawFileFormatVersion: 1, schema: snap.document.schema, records: Object.values(snap.document.store) }
writeFileSync('./platform-admin.tldr', JSON.stringify(file))
console.log('records', file.records.length)
