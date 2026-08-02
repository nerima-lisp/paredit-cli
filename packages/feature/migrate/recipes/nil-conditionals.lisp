;;; One-armed `if' to `when' and `unless'.
;;;
;;; `(if test then nil)' and `(when test then)' evaluate identically in both
;;; Common Lisp and Emacs Lisp -- `when' expands to exactly that `if' -- so
;;; this recipe changes spelling and nothing else. It is here mostly as the
;;; worked example of an ordered recipe: the negated step has to run first,
;;; because the general step would otherwise claim `(if (not p) a nil)' and
;;; produce `(when (not p) a)' where `(unless p a)' was available.
;;;
;;; Deliberately NOT here:
;;;
;;;   (if test nil else)
;;;     `(unless test else)' is equivalent, but the `nil' sits in the *then*
;;;     position, and reading a form whose then-branch is nil as "an unless"
;;;     is a judgement about intent rather than a fact about evaluation. The
;;;     lint rule `one-armed-if' reports it and leaves the decision alone.
;;;
;;;   (if test (progn a b) nil)
;;;     `(when test a b)' is equivalent and shorter, but splicing a `progn'
;;;     body out is a restructuring rather than a rename. `paredit refactor
;;;     flatten-progn' does it, with its own review.
(defmigration
 nil-conditionals
 :description
 "one-armed `if' with a nil else-branch to `when' and `unless'"
 :dialects
 (common-lisp emacs-lisp)
 :steps
 ((:query
   (if (not ?test) ?then
     nil)
   :rewrite
   (unless ?test
     ?then)
   :note
   "first, so the general step below cannot claim a negated test")
  (:query
   (if ?test ?then
     nil)
   :rewrite
   (when ?test
     ?then))))
