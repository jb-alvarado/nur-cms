# Markdown

Text content uses CommonMark with GitHub-Flavored Markdown (GFM) and the extensions listed below.
Raw HTML is escaped, so use Markdown syntax instead of HTML tags.

## Raw HTML compatibility

Raw HTML is not a supported content format. HTML output and the admin preview escape HTML contained in
Markdown. New and revised content should use Markdown and the documented extensions.

For backward compatibility, `output_type=ast` currently retains raw Markdown HTML as nodes with
`type: "html"`. This behavior is deprecated and will be removed in a future release in favor of safer AST
output. Until then, consumers must treat these values as untrusted text and must not insert them into a page
with `innerHTML`, Vue `v-html`, or an equivalent API without a suitable HTML sanitizer.

## Basic Markdown

- Heading: `## Heading`
- Bold: `**bold**`
- Italic: `*italic*`
- Inline code: `` `code` ``
- Link: `[label](https://example.com)`
- Image: `![Alt text](/uploads/image.jpg "Optional title")`
- List: `- First`
- Quote: `> Quote`
- Divider: `---`

````markdown
```rust
let enabled = true;
```
````

## GFM

- Strikethrough: `~~deleted~~`
- Task: `- [x] Done` or `- [ ] Open`
- Autolink: `https://example.com`
- Footnote: `Text[^note]`, then `[^note]: Footnote text`
- Inline footnote: `Text^[Footnote text]`

```markdown
| A   | B   |
| --- | --- |
| 1   | 2   |
```

Footnotes and inline footnotes use one shared numbering sequence.

## Enabled extensions

- Underline: `__underlined__`
- Highlight: `==highlighted==`
- Insert: `++inserted++`
- Superscript: `x^2^`
- Subscript: `H~2~O`
- Subtext: `-# Subtext`
- Spoiler: `||Spoiler text||`

```text
>>>
Multiline quote
>>>

> [!WARNING]
> Alert text

:::notice
Block directive
:::
```

Alerts accept `NOTE`, `TIP`, `IMPORTANT`, `WARNING`, and `CAUTION`. A block directive emits a
`div` whose CSS class is the text after `:::`; use only classes defined by the public frontend.

## API output

`output_type=html` returns safe rendered HTML. `output_type=ast` returns a JSON structure that retains the
extension semantics and, temporarily, the deprecated raw HTML nodes described above. Task items include
`checked`; alerts include `alertType` and `title`; block directives include `class`. Comrak normalizes inline
footnotes into regular footnote references and definitions, and autolinks into regular link nodes.
Footnote identifiers are local to a content node. AST renderers that place several content nodes on one page
must prefix their DOM IDs with a stable node-specific value.

The authenticated admin preview uses `POST /api/markdown/preview`. It accepts a batch of keyed
Markdown nodes and renders them with the same Comrak configuration as public HTML output.
