/* Markdown-lite. The host types a few paragraphs; a guest reads HTML. There is
 * exactly one function in the codebase that turns one into the other, and it
 * builds the markup itself rather than filtering somebody's — every character
 * of the host's text is escaped first, so the only tags in the output are the
 * ones named here. A sanitiser has a list of what to remove and can be wrong
 * about it; this has a list of what it can emit and cannot.
 *
 * What it knows: blank-line paragraphs, `- ` lists, **strong**, _emphasis_,
 * `code`, and bare http(s) links. Nothing nested, no images, no raw HTML. */

const ESCAPES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (char) => ESCAPES[char]!);
}

/* Applied to already-escaped text, so a `<` in the source cannot reach this
 * and no pattern here can produce one that the host wrote. */
function inline(escaped: string): string {
  return escaped
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[\s(])_([^_]+)_(?=[\s.,!?)]|$)/g, '$1<em>$2</em>')
    /* Only the two schemes a browser should follow, and the link says where
     * it goes because the text of it is the URL. */
    .replace(
      /(^|\s)(https?:\/\/[^\s<]+[^\s<.,!?)])/g,
      '$1<a href="$2" rel="noopener noreferrer nofollow">$2</a>',
    );
}

export function renderBody(source: string): string {
  const blocks = source.replace(/\r\n?/g, '\n').trim().split(/\n{2,}/);
  const out: string[] = [];
  for (const block of blocks) {
    const lines = block.split('\n').map((line) => line.trim()).filter((line) => line.length > 0);
    if (lines.length === 0) continue;
    if (lines.every((line) => line.startsWith('- '))) {
      const items = lines.map((line) => `<li>${inline(escapeHtml(line.slice(2)))}</li>`);
      out.push(`<ul>${items.join('')}</ul>`);
      continue;
    }
    out.push(`<p>${inline(escapeHtml(lines.join('\n'))).replace(/\n/g, '<br />')}</p>`);
  }
  return out.join('');
}

/** The first sentence or so, for a list row and a meta description. */
export function excerpt(source: string, max = 140): string {
  const flat = source.replace(/[`*_>#-]/g, '').replace(/\s+/g, ' ').trim();
  return flat.length <= max ? flat : `${flat.slice(0, max - 1).trimEnd()}…`;
}
