# paredit-feature-editor

The editor feature owns the text-buffer aggregate, cursor invariants, editing
actions, undo/redo session state, and the document storage port used by the
interactive command.

The terminal adapter stays in the root application because it is a delivery
mechanism. It depends on this crate through the public `EditorSession` API and
does not reach into the feature's internal state.
