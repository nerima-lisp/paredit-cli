;;; cl.el to cl-lib.
;;;
;;; Emacs 24.3 deprecated the unprefixed names cl.el defined and published
;;; cl-lib with a `cl-' prefix on each. The rename is the whole migration for
;;; the names below: same macro, same function, same arguments.
;;;
;;; Deliberately NOT here:
;;;
;;;   flet, labels, macrolet, letf, letf*
;;;     cl.el's `flet' bound the function cell dynamically; cl-lib's `cl-flet'
;;;     is lexical, as Common Lisp's is. Code that relied on the dynamic
;;;     binding -- which is why a lot of it used `flet' in the first place --
;;;     breaks under the rename with no error. A prefix is not the migration
;;;     for these; reading the call site is.
;;;
;;;   first, second ... tenth, rest
;;;     `cl-first' exists, but so does the possibility that the file defines
;;;     its own `first'. These read as ordinary function calls rather than as
;;;     cl.el's macros, so the rename is safe only after checking there is no
;;;     local definition -- which `paredit inspect find-symbol' answers and a
;;;     blind rewrite does not.
;;;
;;;   values, values-list, multiple-value-setq
;;;     cl-lib's versions are documented as approximations of the Common Lisp
;;;     behaviour rather than as equivalents.
;;;
;;;   lambda*
;;;     cl.el's `lambda*' has no one-symbol cl-lib spelling. The replacement is
;;;     `(cl-function (lambda ...))', which restructures the form rather than
;;;     renaming its head, and a recipe step can only rename.
;;;
;;;   dolist, dotimes
;;;     these are plain Emacs Lisp, not cl.el. `cl-dolist' and `cl-dotimes'
;;;     exist but are a different, extended macro; renaming into them widens
;;;     what the code accepts for no reason.
;;;
;;; Every step matches head position only, so a variable named `incf' or a
;;; quoted symbol `'case' is left alone.
(defmigration elisp-cl-lib
  :description "cl.el's unprefixed names to their cl-lib `cl-' spellings"
  :dialects (emacs-lisp)
  :steps (;; Places.
          (:query (incf ?args...)     :rewrite (cl-incf ?args...))
          (:query (decf ?args...)     :rewrite (cl-decf ?args...))
          (:query (pushnew ?args...)  :rewrite (cl-pushnew ?args...))
          (:query (getf ?args...)     :rewrite (cl-getf ?args...))
          (:query (remf ?args...)     :rewrite (cl-remf ?args...))
          (:query (rotatef ?args...)  :rewrite (cl-rotatef ?args...))
          (:query (shiftf ?args...)   :rewrite (cl-shiftf ?args...))

          ;; Control flow.
          (:query (case ?args...)     :rewrite (cl-case ?args...))
          (:query (ecase ?args...)    :rewrite (cl-ecase ?args...))
          (:query (typecase ?args...) :rewrite (cl-typecase ?args...))
          (:query (etypecase ?args...) :rewrite (cl-etypecase ?args...))
          (:query (block ?args...)    :rewrite (cl-block ?args...))
          (:query (return ?args...)   :rewrite (cl-return ?args...))
          (:query (return-from ?args...) :rewrite (cl-return-from ?args...))
          (:query (loop ?args...)     :rewrite (cl-loop ?args...)
           :note "cl-loop is the same extended loop macro under a new name")
          (:query (do ?args...)       :rewrite (cl-do ?args...))
          (:query (do* ?args...)      :rewrite (cl-do* ?args...))

          ;; Binding and definition.
          (:query (destructuring-bind ?args...) :rewrite (cl-destructuring-bind ?args...))
          (:query (multiple-value-bind ?args...) :rewrite (cl-multiple-value-bind ?args...))
          (:query (defstruct ?args...) :rewrite (cl-defstruct ?args...))
          (:query (defun* ?args...)   :rewrite (cl-defun ?args...))
          (:query (defmacro* ?args...) :rewrite (cl-defmacro ?args...))
          (:query (function* ?args...) :rewrite (cl-function ?args...))

          ;; Assertions.
          (:query (assert ?args...)     :rewrite (cl-assert ?args...))
          (:query (check-type ?args...) :rewrite (cl-check-type ?args...))

          ;; Sequences and sets.
          (:query (remove-if ?args...)        :rewrite (cl-remove-if ?args...))
          (:query (remove-if-not ?args...)    :rewrite (cl-remove-if-not ?args...))
          (:query (delete-if ?args...)        :rewrite (cl-delete-if ?args...))
          (:query (delete-if-not ?args...)    :rewrite (cl-delete-if-not ?args...))
          (:query (find-if ?args...)          :rewrite (cl-find-if ?args...))
          (:query (position-if ?args...)      :rewrite (cl-position-if ?args...))
          (:query (count-if ?args...)         :rewrite (cl-count-if ?args...))
          (:query (member-if ?args...)        :rewrite (cl-member-if ?args...))
          (:query (remove-duplicates ?args...) :rewrite (cl-remove-duplicates ?args...))
          (:query (delete-duplicates ?args...) :rewrite (cl-delete-duplicates ?args...))
          (:query (subseq ?args...)           :rewrite (cl-subseq ?args...))
          (:query (reduce ?args...)           :rewrite (cl-reduce ?args...))
          (:query (some ?args...)             :rewrite (cl-some ?args...))
          (:query (every ?args...)            :rewrite (cl-every ?args...))
          (:query (notany ?args...)           :rewrite (cl-notany ?args...))
          (:query (notevery ?args...)         :rewrite (cl-notevery ?args...))
          (:query (adjoin ?args...)           :rewrite (cl-adjoin ?args...))
          (:query (union ?args...)            :rewrite (cl-union ?args...))
          (:query (intersection ?args...)     :rewrite (cl-intersection ?args...))
          (:query (set-difference ?args...)   :rewrite (cl-set-difference ?args...))
          (:query (coerce ?args...)           :rewrite (cl-coerce ?args...))

          ;; Predicates.
          (:query (evenp ?args...)  :rewrite (cl-evenp ?args...))
          (:query (oddp ?args...)   :rewrite (cl-oddp ?args...))
          (:query (plusp ?args...)  :rewrite (cl-plusp ?args...))
          (:query (minusp ?args...) :rewrite (cl-minusp ?args...))))
