#lang racket/base
;; The dangerous twin of `clean.rkt`: valid Racket that compiles and runs, in
;; which each rule in this package fires exactly once.
;;
;; Every defect here is the *executed* premise from the corresponding domain
;; module, not an invented shape.
(require racket/match)

;; racket-match-unreachable-clause: the `[_ 'other]` clause matches every
;; value, so `[(? string?) 'str]` below it can never run. `raco make` compiles
;; this without a word and `(classify "hi")` returns 'other.
(define
 (classify value)
 (match value [(? number?) 'number] [_ 'other] [(? string?) 'string]))

;; racket-begin0-single-form: with one form there is no sequence, so this is
;; exactly `(compute-total)`.
(define (total) (begin0 (compute-total)))

;; racket-case-lambda-single-clause: one clause dispatches on nothing; this is
;; `(lambda (x) (* x 2))` written the long way.
(define double (case-lambda [(x) (* x 2)]))

;; racket-parameterize-empty-bindings: rebinds no parameter, so it is exactly
;; its own body.
(define (announce message) (parameterize () (displayln message)))

;; racket-for-comprehension-value-discarded: the list is built, filled and
;; dropped, because it is not the last form of the body. `for` does the same
;; iteration without allocating it.
(define
 (report items)
 (for/list ([item (in-list items)]) (displayln item))
 'done)

(define (compute-total) 42)
