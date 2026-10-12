# kami

A simple Markdown viewer and editor where reading and writing happen in the same pane.

Most Markdown tools split the screen: raw source on one side, rendered preview on the other. kami shows one document. It looks rendered, and you edit it in place. Click into the text and you're editing it. No mode switch and no second pane.

> **Status:** early planning. No code yet. See [REQUIREMENTS.md](REQUIREMENTS.md) for the working spec and open questions.

## Goals

- **One pane.** The document you read is the document you edit.
- **Plain files.** kami opens and saves ordinary `.md` files. No proprietary format, no lock-in, no hidden metadata.
- **Simple.** A small, focused tool, not a notes app or a knowledge base.
- **Faithful.** Saving a file you didn't change leaves it byte-for-byte identical. Edits touch only what you edited.

## Non-goals (for now)

- Folder or project sidebar (kami opens individual files)
- Sync, accounts, or cloud storage
- Plugins or extension APIs
- Collaboration or real-time multi-user editing
- WYSIWYG features that have no Markdown equivalent

## Getting started

_TBD once the platform and stack are chosen._

## License

_TBD_
