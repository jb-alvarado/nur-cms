# Import Markdown Files

NUR CMS can import markdown files and associated media into the database.

## Options

| Option | Description |
|--------|-------------|
| `--import-media <PATH>` | Directory path to import images from. These images are referenced by markdown files. |
| `--import-markdown <PATH>` | Directory path for recursive import of multiple files, or path to a single file. |
| `--ignore-files <PATTERN>` | Comma-separated list of filenames (without path) to exclude from import. |

## Example

```bash
nur-cms \
  --import-media ~/content/images \
  --import-markdown ~/content/articles \
  --ignore-files _index.md,draft.md
```

After running the command above, prompts will appear asking how existing entries should be
handled and which content type, author, and locale should be used.

An existing entry is identified by its content type, locale, and slug. The importer can either
update that entry or retain the previous behavior and create another entry with a random suffix
added to its slug. Use an explicit, stable `slug` in the frontmatter for repeatable updates:

```yaml
---
title: Example article
slug: example-article
date: 2008-04-17
---
```
