# paredit-feature-lint-safety

Lint rules about what a program does to the world outside the form being read:
global state two threads can reach, input that becomes code, streams that
outlive their `unwind-protect`, and errors that are caught and then dropped.

Four categories share the package because they share a shape. Each rule here
reports a form that is locally correct — the `setf` assigns, the `read` reads,
the `handler-case` catches — and whose defect is only visible one level out:
another thread, an untrusted caller, a non-local exit, a condition nobody will
now hear about.

That is also why almost none of them is fixable. The repair for a swallowed
error is to decide what should happen instead, and no rule knows.
