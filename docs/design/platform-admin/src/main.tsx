import { createRoot } from 'react-dom/client'
import { Tldraw, type Editor } from 'tldraw'
import { getAssetUrlsByImport } from '@tldraw/assets/imports.vite'
import 'tldraw/tldraw.css'
import { built } from './board'

// The stock icon map points every icon at `0_merged.svg#name`; a fragment on a
// data: URI never targets, so once inlined every icon rendered as the whole
// sprite. Use the individual files instead (inlined by vite as data: URIs).
const individualIcons = import.meta.glob('../node_modules/@tldraw/assets/icons/icon/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>
const icons: Record<string, string> = {}
for (const [path, url] of Object.entries(individualIcons)) {
  const name = path.split('/').pop()!.replace(/\.svg$/, '')
  // vite percent-encodes inlined SVGs and swaps " for ', which breaks inside
  // an unquoted CSS `mask: url(...)`; re-encode as base64 so the mask loads.
  const safe = url.startsWith('data:image/svg+xml,')
    ? 'data:image/svg+xml;base64,' + btoa(decodeURIComponent(url.slice('data:image/svg+xml,'.length)))
    : url
  if (name !== '0_merged') icons[name] = safe
}
const base = getAssetUrlsByImport()
const assetUrls = { ...base, icons: { ...base.icons, ...icons } }

function onMount(editor: Editor) {
  ;(window as unknown as { editor: Editor }).editor = editor
  editor.createShapes(built.shapes)
  editor.createBindings(built.bindings)
  editor.selectNone()
  editor.zoomToFit({ immediate: true })
  editor.setCurrentTool('hand')
}

createRoot(document.getElementById('root')!).render(
    <div style={{ position: 'fixed', inset: 0 }}>
      <Tldraw assetUrls={assetUrls} onMount={onMount} inferDarkMode />
    </div>
)
