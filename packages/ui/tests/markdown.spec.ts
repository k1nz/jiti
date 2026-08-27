import { describe, expect, it } from 'vitest';
import { escapeHtml, renderReviewMarkdown } from '../src/markdown';

describe('renderReviewMarkdown', () => {
  it('把标题、列表和加粗渲染成 HTML，而不是源码', () => {
    const html = renderReviewMarkdown(
      '## 高频错误\n\n主谓不一致最常见。\n\n- 第三人称加 **s**\n- 时态一致',
    );
    expect(html).toContain('<h2>高频错误</h2>');
    expect(html).toContain('<p>主谓不一致最常见。</p>');
    expect(html).toContain('<ul>');
    expect(html).toContain('<strong>s</strong>');
    expect(html).not.toContain('## 高频错误');
    expect(html).not.toContain('- 第三人称');
  });

  it('转义 HTML 注入', () => {
    const html = renderReviewMarkdown('注意 <script>alert(1)</script> 与 `a<b`');
    expect(html).not.toContain('<script>');
    expect(html).toContain('&lt;script&gt;');
    expect(html).toContain('<code>a&lt;b</code>');
    expect(escapeHtml('<x>')).toBe('&lt;x&gt;');
  });

  it('空内容得到空字符串', () => {
    expect(renderReviewMarkdown('')).toBe('');
    expect(renderReviewMarkdown('\n')).toBe('');
  });
});
