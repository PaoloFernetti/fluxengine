import MarkdownIt from 'markdown-it'
import anchor from 'markdown-it-anchor'
import taskLists from 'markdown-it-task-lists'
import hljs from 'highlight.js/lib/core'
import rust from 'highlight.js/lib/languages/rust'
import javascript from 'highlight.js/lib/languages/javascript'
import json from 'highlight.js/lib/languages/json'
import bash from 'highlight.js/lib/languages/bash'
import ini from 'highlight.js/lib/languages/ini'
import xml from 'highlight.js/lib/languages/xml'

hljs.registerLanguage('rust', rust)
hljs.registerLanguage('javascript', javascript)
hljs.registerLanguage('js', javascript)
hljs.registerLanguage('json', json)
hljs.registerLanguage('bash', bash)
hljs.registerLanguage('shell', bash)
hljs.registerLanguage('bat', bash)
hljs.registerLanguage('ini', ini)
hljs.registerLanguage('toml', ini)
hljs.registerLanguage('xml', xml)
hljs.registerLanguage('html', xml)

export function slugify(text) {
  return String(text)
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
}

const md = new MarkdownIt({
  html: true,
  linkify: true,
  typographer: true,
  breaks: false,
})

md.use(anchor, {
  slugify,
  level: [2, 3],
  permalink: anchor.permalink.linkInsideHeader({
    symbol: '#',
    placement: 'before',
    ariaHidden: true,
  }),
})

md.use(taskLists, {
  enabled: false,
  label: true,
  labelAfter: true,
})

md.renderer.rules.fence = (tokens, idx) => {
  const token = tokens[idx]
  const info = token.info.trim()
  const content = token.content

  if (info === 'mermaid') {
    return `<div class="mermaid">${md.utils.escapeHtml(content)}</div>\n`
  }

  if (info && hljs.getLanguage(info)) {
    const highlighted = hljs.highlight(content, { language: info, ignoreIllegals: true }).value
    return `<pre class="hljs" data-lang="${info}"><code class="language-${info}">${highlighted}</code></pre>\n`
  }

  return `<pre class="hljs" data-lang="text"><code>${md.utils.escapeHtml(content)}</code></pre>\n`
}

export { md }
