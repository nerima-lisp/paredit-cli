#lang racket/base
;;; The dangerous twin of clean.rkt, in bracket style: each rule Racket is in
;;; scope for fires exactly once. `scheme-memq-assq-literal-key` is absent on
;;; purpose -- it is Scheme-only, because Racket specifies the fixnum and
;;; character cases R7RS leaves open.
;; scheme-begin-single-form
(define (start!) (begin (open-connection)))

;; scheme-let-star-independent-bindings, bracketed
(define
 (window-size)
 (let* ([width 80]
        [height 24])
   (cons width height)))

;; scheme-named-let-never-recurs, bracketed binding list
(define
 (first-item items)
 (let scan
   ([rest items])
   (car rest)))
