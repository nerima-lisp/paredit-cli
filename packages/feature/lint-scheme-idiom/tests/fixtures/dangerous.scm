;;; The dangerous twin of clean.scm: the same shapes, each one broken in the
;;; single way its rule is about, exactly once. If a rule stops firing here the
;;; clean-corpus test can no longer be passing for the right reason.
;; scheme-begin-single-form: one body form, so the begin does nothing.
(define (start!) (begin (open-connection)))

;; scheme-let-star-independent-bindings: two literal initializers, neither of
;; which can see the other.
(define
 (window-size)
 (let* ((width 80)
        (height 24))
   (cons width height)))

;; scheme-memq-assq-literal-key: R7RS 6.4 spells this exact search out as
;; unspecified, and gives `memv` as the one that is not.
(define (known-code? codes) (memq 101 codes))

;; scheme-named-let-never-recurs: `scan` is bound and never called.
(define
 (first-line port)
 (let scan
   ((line (read-line port)))
   line))
