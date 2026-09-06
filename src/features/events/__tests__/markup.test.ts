import { describe, expect, it } from 'vitest';

import { excerpt, renderBody } from '../markup';

describe('renderBody', () => {
  it('makes paragraphs out of blank lines', () => {
    expect(renderBody('One.\n\nTwo.')).toBe('<p>One.</p><p>Two.</p>');
  });

  it('makes a list out of dashes', () => {
    expect(renderBody('- snacks\n- a game')).toBe('<ul><li>snacks</li><li>a game</li></ul>');
  });

  it('knows strong, emphasis and code', () => {
    expect(renderBody('**bring** _a_ `cake`')).toBe(
      '<p><strong>bring</strong> <em>a</em> <code>cake</code></p>',
    );
  });

  it('cannot emit a tag the host wrote', () => {
    const html = renderBody('<script>alert(1)</script> and <b>bold</b>');
    expect(html).not.toContain('<script');
    expect(html).not.toContain('<b>');
    expect(html).toContain('&lt;script&gt;');
  });

  it('does not let an attribute escape a link', () => {
    const html = renderBody('https://example.com/"onmouseover="x');
    expect(html).not.toContain('onmouseover="x"');
    expect(html).toContain('&quot;');
  });

  it('links bare http(s) URLs and nothing else', () => {
    expect(renderBody('see https://ronitnath.com/e/x for the map')).toContain(
      '<a href="https://ronitnath.com/e/x" rel="noopener noreferrer nofollow">',
    );
    expect(renderBody('javascript:alert(1)')).not.toContain('<a ');
  });

  it('is empty for an empty body', () => {
    expect(renderBody('   \n\n  ')).toBe('');
  });
});

describe('excerpt', () => {
  it('flattens and bounds', () => {
    expect(excerpt('**Board** games\n\nand hanging out')).toBe('Board games and hanging out');
    expect(excerpt('x'.repeat(200), 20)).toHaveLength(20);
  });
});
