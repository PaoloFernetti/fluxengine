import mermaid from 'mermaid'

let initializedFor = null

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&')
    .replace(/</g, '<')
    .replace(/>/g, '>')
}

export function initMermaid(theme) {
  if (initializedFor === theme) return
  initializedFor = theme
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: 'loose',
    theme: theme === 'dark' ? 'dark' : 'default',
    fontFamily:
      'Inter, "Segoe UI", system-ui, -apple-system, BlinkMacSystemFont, sans-serif',
    flowchart: { htmlLabels: true, curve: 'basis', padding: 12 },
    sequence: { mirrorActors: false, showSequenceNumbers: false },
    er: { useMaxWidth: true },
  })
}

export async function renderMermaid(root, theme) {
  if (!root) return
  initMermaid(theme)

  const nodes = [...root.querySelectorAll('.mermaid')]
  for (const node of nodes) {
    if (!node.dataset.src) node.dataset.src = node.textContent
    node.removeAttribute('data-processed')
    node.innerHTML = node.dataset.src
  }

  for (const node of nodes) {
    try {
      await mermaid.run({ nodes: [node], suppressErrors: true })
    } catch {
      node.innerHTML = `<pre class="mermaid-error">${escapeHtml(node.dataset.src)}</pre>`
    }
  }
}
